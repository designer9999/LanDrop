use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time;
use walkdir::WalkDir;

use super::protocol::{Message, CHUNK_SIZE, TCP_PORT};
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use crate::commands::format_size;
use crate::path_utils::sanitize_relative_path;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_IO_TIMEOUT: Duration = Duration::from_secs(30);
const FILE_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CONTROL_FRAME_BYTES: usize = 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_TRANSFER_PATH_BYTES: usize = 4096;
const MAX_BATCH_FILES: u32 = 100_000;
const DISK_SPACE_RESERVE_BYTES: u64 = 10_000_000;
const PART_FILE_CREATE_ATTEMPTS: usize = 16;

static RECEIVE_FINALIZE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub struct Connection {
    reader: Mutex<OwnedReadHalf>,
    writer: Mutex<OwnedWriteHalf>,
}

impl Connection {
    fn from_stream(stream: TcpStream) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            reader: Mutex::new(reader),
            writer: Mutex::new(writer),
        }
    }

    /// Accept incoming connection: read sender's UUID, send our UUID back.
    /// Returns (connection, sender_uuid_string).
    pub async fn from_incoming(
        mut stream: TcpStream,
        my_uuid: &[u8; 16],
    ) -> Result<(Arc<Self>, String), String> {
        stream.set_nodelay(true).map_err(|e| e.to_string())?;

        let peer_uuid = time::timeout(HANDSHAKE_TIMEOUT, async {
            // Read sender's UUID and return ours. This identifies a peer but does
            // not cryptographically authenticate it.
            let mut peer_uuid = [0u8; 16];
            stream
                .read_exact(&mut peer_uuid)
                .await
                .map_err(|e| format!("Failed to read peer identity: {e}"))?;
            stream
                .write_all(my_uuid)
                .await
                .map_err(|e| format!("Failed to send local identity: {e}"))?;
            stream
                .flush()
                .await
                .map_err(|e| format!("Failed to flush identity handshake: {e}"))?;
            Ok::<[u8; 16], String>(peer_uuid)
        })
        .await
        .map_err(|_| "Incoming identity handshake timed out".to_string())??;

        let sender_id = uuid::Uuid::from_bytes(peer_uuid).to_string();
        Ok((Arc::new(Self::from_stream(stream)), sender_id))
    }

    /// Connect to a peer: send our UUID and verify the UUID they return.
    pub async fn from_outgoing(
        mut stream: TcpStream,
        my_uuid: &[u8; 16],
        expected_peer_uuid: &[u8; 16],
    ) -> Result<Arc<Self>, String> {
        stream.set_nodelay(true).map_err(|e| e.to_string())?;

        let peer_uuid = time::timeout(HANDSHAKE_TIMEOUT, async {
            stream
                .write_all(my_uuid)
                .await
                .map_err(|e| format!("Failed to send local identity: {e}"))?;
            stream
                .flush()
                .await
                .map_err(|e| format!("Failed to flush identity handshake: {e}"))?;

            let mut peer_uuid = [0u8; 16];
            stream
                .read_exact(&mut peer_uuid)
                .await
                .map_err(|e| format!("Failed to read peer identity: {e}"))?;
            Ok::<[u8; 16], String>(peer_uuid)
        })
        .await
        .map_err(|_| "Outgoing identity handshake timed out".to_string())??;

        if peer_uuid != *expected_peer_uuid {
            let expected = uuid::Uuid::from_bytes(*expected_peer_uuid);
            let received = uuid::Uuid::from_bytes(peer_uuid);
            return Err(format!(
                "Peer identity mismatch: expected {expected}, received {received}"
            ));
        }

        Ok(Arc::new(Self::from_stream(stream)))
    }

    pub async fn send_message(&self, msg: &Message) -> Result<(), String> {
        let mut w = self.writer.lock().await;
        write_message(&mut w, msg).await?;
        time::timeout(CONTROL_IO_TIMEOUT, w.flush())
            .await
            .map_err(|_| "Timed out flushing control message".to_string())?
            .map_err(|e| format!("Failed to flush control message: {e}"))?;
        Ok(())
    }

    /// Read one framed message.
    ///
    /// `Ok(None)` means the peer closed cleanly before sending any byte of the
    /// next frame. A partial length, partial payload, timeout, invalid JSON, or
    /// invalid message is an error.
    pub async fn recv_message(&self) -> Result<Option<Message>, String> {
        let mut r = self.reader.lock().await;
        let mut len_buf = [0u8; 4];

        let first_byte_count = time::timeout(CONTROL_IO_TIMEOUT, r.read(&mut len_buf[..1]))
            .await
            .map_err(|_| "Timed out waiting for a control message".to_string())?
            .map_err(|e| format!("Failed to read control frame: {e}"))?;
        if first_byte_count == 0 {
            return Ok(None);
        }

        time::timeout(CONTROL_IO_TIMEOUT, r.read_exact(&mut len_buf[1..]))
            .await
            .map_err(|_| "Timed out reading control frame length".to_string())?
            .map_err(|e| format!("Truncated control frame length: {e}"))?;

        let len = u32::from_be_bytes(len_buf) as usize;

        if len == 0 {
            return Err("Empty control frame".into());
        }
        if len > MAX_CONTROL_FRAME_BYTES {
            return Err(format!(
                "Control message too large: {len} bytes (maximum {MAX_CONTROL_FRAME_BYTES})"
            ));
        }

        let mut buf = vec![0u8; len];
        time::timeout(CONTROL_IO_TIMEOUT, r.read_exact(&mut buf))
            .await
            .map_err(|_| "Timed out reading control frame payload".to_string())?
            .map_err(|e| format!("Truncated control frame payload: {e}"))?;

        let message: Message =
            serde_json::from_slice(&buf).map_err(|e| format!("Invalid control message: {e}"))?;
        validate_message(&message)?;
        Ok(Some(message))
    }

    pub async fn recv_raw(&self, buf: &mut [u8]) -> Result<usize, String> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut r = self.reader.lock().await;
        let read = time::timeout(FILE_IDLE_TIMEOUT, r.read(buf))
            .await
            .map_err(|_| "File transfer timed out waiting for data".to_string())?
            .map_err(|e| format!("Failed to receive file data: {e}"))?;
        if read == 0 {
            return Err("Peer closed the connection before the file was complete".into());
        }
        Ok(read)
    }
}

fn validate_message(msg: &Message) -> Result<(), String> {
    match msg {
        Message::Text { text } if text.len() > MAX_TEXT_BYTES => Err(format!(
            "Text message too large: {} bytes (maximum {MAX_TEXT_BYTES})",
            text.len()
        )),
        Message::File { name, .. } | Message::Dir { name }
            if name.len() > MAX_TRANSFER_PATH_BYTES =>
        {
            Err(format!(
                "Transfer path too long: {} bytes (maximum {MAX_TRANSFER_PATH_BYTES})",
                name.len()
            ))
        }
        Message::Batch { count } if *count > MAX_BATCH_FILES => Err(format!(
            "Batch contains too many files: {count} (maximum {MAX_BATCH_FILES})"
        )),
        _ => Ok(()),
    }
}

fn encode_message(msg: &Message) -> Result<([u8; 4], Vec<u8>), String> {
    validate_message(msg)?;
    let json = serde_json::to_vec(msg).map_err(|e| format!("Failed to encode message: {e}"))?;
    if json.len() > MAX_CONTROL_FRAME_BYTES {
        return Err(format!(
            "Control message too large: {} bytes (maximum {MAX_CONTROL_FRAME_BYTES})",
            json.len()
        ));
    }
    let len = u32::try_from(json.len())
        .map_err(|_| "Control message length exceeds protocol capacity".to_string())?
        .to_be_bytes();
    Ok((len, json))
}

/// Write a framed message directly to a writer (caller already holds the lock)
async fn write_message(
    w: &mut tokio::net::tcp::OwnedWriteHalf,
    msg: &Message,
) -> Result<(), String> {
    let (len, json) = encode_message(msg)?;
    time::timeout(CONTROL_IO_TIMEOUT, async {
        w.write_all(&len).await?;
        w.write_all(&json).await
    })
    .await
    .map_err(|_| "Timed out sending control message".to_string())?
    .map_err(|e| format!("Failed to send control message: {e}"))
}

async fn write_file_chunk(w: &mut OwnedWriteHalf, bytes: &[u8]) -> Result<(), String> {
    time::timeout(FILE_IDLE_TIMEOUT, w.write_all(bytes))
        .await
        .map_err(|_| "File transfer timed out while sending data".to_string())?
        .map_err(|e| format!("Failed to send file data: {e}"))
}

// ─── On-demand send functions ───

/// Open a TCP connection to a peer, verify the expected public UUID, send text, close.
pub async fn send_text_to_peer(
    peer_ip: &str,
    my_uuid: &[u8; 16],
    expected_peer_uuid: &[u8; 16],
    text: &str,
) -> Result<(), String> {
    let addr: SocketAddr = format!("{}:{}", peer_ip, TCP_PORT)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let stream = time::timeout(Duration::from_secs(3), TcpStream::connect(addr))
        .await
        .map_err(|_| format!("Connection timeout to {}", addr))?
        .map_err(|e| format!("Cannot connect to {}: {}", addr, e))?;

    let conn = Connection::from_outgoing(stream, my_uuid, expected_peer_uuid).await?;
    conn.send_message(&Message::Text {
        text: text.to_string(),
    })
    .await?;
    conn.send_message(&Message::Done).await?;
    Ok(())
}

/// Open the LanDrop TCP handshake only and return the peer UUID.
pub async fn probe_peer_id(peer_ip: &str, my_uuid: &[u8; 16]) -> Result<String, String> {
    let addr: SocketAddr = format!("{}:{}", peer_ip, TCP_PORT)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    time::timeout(Duration::from_millis(700), async {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| format!("Cannot connect to {}: {}", addr, e))?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        stream.write_all(my_uuid).await.map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;

        let mut peer_uuid = [0u8; 16];
        stream
            .read_exact(&mut peer_uuid)
            .await
            .map_err(|e| e.to_string())?;
        Ok(uuid::Uuid::from_bytes(peer_uuid).to_string())
    })
    .await
    .map_err(|_| format!("Probe timeout to {}", addr))?
}

/// Open a TCP connection to a peer, verify the expected public UUID, send files, close.
pub async fn send_files_to_peer(
    peer_ip: &str,
    my_uuid: &[u8; 16],
    expected_peer_uuid: &[u8; 16],
    paths: &[String],
    handle: Option<&AppHandle>,
) -> Result<(), String> {
    // Collect all files to send (skip symlinks for security)
    let mut file_entries: Vec<(String, PathBuf)> = Vec::new();

    for path_str in paths {
        let path = Path::new(path_str);

        if path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }

        if path.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("folder")
                .to_string();

            for entry in WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    let rel = entry.path().strip_prefix(path).unwrap_or(entry.path());
                    let name = format!("{}/{}", dir_name, rel.to_string_lossy().replace('\\', "/"));
                    file_entries.push((name, entry.path().to_path_buf()));
                }
            }
        } else if path.is_file() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            file_entries.push((name, path.to_path_buf()));
        } else {
            // File doesn't exist or can't be accessed — report error instead of silently skipping
            if let Some(h) = handle {
                let _ = h.emit(
                    "lan_log",
                    serde_json::json!({
                        "level": "error",
                        "text": format!("File not accessible: {}", path.display()),
                    }),
                );
            }
            return Err(format!("File not accessible: {}", path.display()));
        }
    }

    if file_entries.is_empty() {
        return Err("No files to send".into());
    }

    let batch_count = u32::try_from(file_entries.len())
        .map_err(|_| "Too many files for one transfer".to_string())?;
    if batch_count > MAX_BATCH_FILES {
        return Err(format!(
            "Batch contains too many files: {batch_count} (maximum {MAX_BATCH_FILES})"
        ));
    }

    // Calculate total size for progress
    let total_size: u64 = {
        let mut total = 0u64;
        for (_, fp) in &file_entries {
            let metadata = tokio::fs::metadata(fp)
                .await
                .map_err(|e| format!("Failed to read metadata for {}: {e}", fp.display()))?;
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| "Combined transfer size exceeds protocol capacity".to_string())?;
        }
        total
    };
    let total_files = file_entries.len();

    // Connect to peer
    let addr: SocketAddr = format!("{}:{}", peer_ip, TCP_PORT)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let stream = time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .map_err(|_| "Connection timeout".to_string())?
        .map_err(|e| e.to_string())?;

    let conn = Connection::from_outgoing(stream, my_uuid, expected_peer_uuid).await?;

    if let Some(h) = handle {
        let _ = h.emit(
            "lan_transfer_progress",
            serde_json::json!({
                "direction": "send",
                "phase": "start",
                "total_bytes": total_size,
                "total_files": total_files,
                "sent_bytes": 0,
                "sent_files": 0,
            }),
        );
    }

    // Use writer lock for atomic sends (no interleaving with concurrent operations)
    let mut w = conn.writer.lock().await;

    // Always send batch header so receiver knows what to expect
    write_message(&mut w, &Message::Batch { count: batch_count }).await?;

    let mut sent_bytes: u64 = 0;
    let mut sent_files: usize = 0;

    for (name, file_path) in &file_entries {
        // Open first, then obtain metadata from that same handle so the declared
        // size and streamed bytes refer to the same file instance.
        let mut file = tokio::fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open {}: {e}", file_path.display()))?;
        let metadata = file
            .metadata()
            .await
            .map_err(|e| format!("Failed to read metadata for {}: {e}", file_path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "Transfer source is no longer a regular file: {}",
                file_path.display()
            ));
        }
        let size = metadata.len();

        // Send dir marker
        if let Some(last_sep) = name.rfind('/') {
            let dir_part = &name[..last_sep];
            write_message(
                &mut w,
                &Message::Dir {
                    name: dir_part.to_string(),
                },
            )
            .await?;
        }

        // Send file header + raw data
        write_message(
            &mut w,
            &Message::File {
                name: name.clone(),
                size,
            },
        )
        .await?;

        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut remaining = size;

        while remaining > 0 {
            let to_read = remaining.min(CHUNK_SIZE as u64) as usize;
            let n = file
                .read(&mut buf[..to_read])
                .await
                .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;
            if n == 0 {
                return Err(format!(
                    "Source file ended early while sending {} ({} bytes still expected)",
                    file_path.display(),
                    remaining
                ));
            }
            write_file_chunk(&mut w, &buf[..n]).await?;
            remaining -= n as u64;
            sent_bytes = sent_bytes
                .checked_add(n as u64)
                .ok_or_else(|| "Sent byte counter overflowed".to_string())?;

            if let Some(h) = handle {
                let _ = h.emit(
                    "lan_transfer_progress",
                    serde_json::json!({
                        "direction": "send",
                        "phase": "transferring",
                        "total_bytes": total_size,
                        "total_files": total_files,
                        "sent_bytes": sent_bytes,
                        "sent_files": sent_files,
                        "current_file": name,
                    }),
                );
            }
        }

        time::timeout(FILE_IDLE_TIMEOUT, w.flush())
            .await
            .map_err(|_| "File transfer timed out while flushing data".to_string())?
            .map_err(|e| format!("Failed to flush file data: {e}"))?;
        sent_files += 1;
    }

    // Always send Done so receiver knows the transfer is complete
    write_message(&mut w, &Message::Done).await?;
    time::timeout(CONTROL_IO_TIMEOUT, w.flush())
        .await
        .map_err(|_| "Timed out flushing transfer completion".to_string())?
        .map_err(|e| format!("Failed to flush transfer completion: {e}"))?;

    drop(w);

    if let Some(h) = handle {
        let _ = h.emit(
            "lan_transfer_progress",
            serde_json::json!({
                "direction": "send",
                "phase": "done",
                "total_bytes": total_size,
                "total_files": total_files,
                "sent_bytes": sent_bytes,
                "sent_files": sent_files,
            }),
        );
    }

    Ok(())
}

// ─── Receive functions (called by incoming TCP handler) ───

pub async fn receive_file(
    conn: &Connection,
    name: &str,
    size: u64,
    out_folder: &str,
    sort_by_date: bool,
    handle: Option<&AppHandle>,
) -> Result<String, String> {
    let safe_name = sanitize_relative_path(name);
    let base_dir = resolve_receive_base_dir(out_folder, sort_by_date);
    let desired_out_path = base_dir.join(&safe_name);
    let parent = desired_out_path
        .parent()
        .ok_or_else(|| "Receive path has no parent directory".to_string())?;
    let canonical_base = ensure_receive_directory(&base_dir, parent).await?;

    if let Ok(canonical_out) = desired_out_path.canonicalize() {
        if !canonical_out.starts_with(&canonical_base) {
            return Err("Path traversal detected".into());
        }
    }

    check_disk_space(&desired_out_path, size)?;

    let (mut file, partial_path) = create_partial_file(parent).await?;
    let mut remaining = size;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut received_bytes: u64 = 0;

    let receive_result: Result<(), String> = async {
        while remaining > 0 {
            let to_read = remaining.min(CHUNK_SIZE as u64) as usize;
            let read = conn.recv_raw(&mut buf[..to_read]).await?;
            file.write_all(&buf[..read])
                .await
                .map_err(|e| format!("Failed to write received file: {e}"))?;
            remaining -= read as u64;
            received_bytes = received_bytes
                .checked_add(read as u64)
                .ok_or_else(|| "Received byte counter overflowed".to_string())?;

            if let Some(h) = handle {
                let _ = h.emit(
                    "lan_transfer_progress",
                    serde_json::json!({
                        "direction": "receive",
                        "phase": "transferring",
                        "total_bytes": size,
                        "received_bytes": received_bytes,
                        "current_file": name,
                    }),
                );
            }
        }

        file.flush()
            .await
            .map_err(|e| format!("Failed to flush received file: {e}"))
    }
    .await;

    if let Err(error) = receive_result {
        drop(file);
        let _ = tokio::fs::remove_file(&partial_path).await;
        return Err(error);
    }

    // Close the handle before renaming; Windows does not permit renaming an open
    // file in all configurations.
    drop(file);

    let out_path = match finalize_partial_file(&partial_path, &desired_out_path).await {
        Ok(path) => path,
        Err(error) => {
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err(error);
        }
    };

    Ok(out_path.to_string_lossy().to_string())
}

pub async fn receive_batch(
    conn: &Connection,
    count: u32,
    out_folder: &str,
    sort_by_date: bool,
    handle: Option<&AppHandle>,
) -> Result<Vec<(String, String, u64)>, String> {
    if count > MAX_BATCH_FILES {
        return Err(format!(
            "Batch contains too many files: {count} (maximum {MAX_BATCH_FILES})"
        ));
    }

    let mut files = Vec::with_capacity(count as usize);

    if let Some(h) = handle {
        let _ = h.emit(
            "lan_transfer_progress",
            serde_json::json!({
                "direction": "receive",
                "phase": "start",
                "total_files": count,
                "received_files": 0,
            }),
        );
    }

    for file_index in 0..count {
        let msg = recv_required_message(conn, "batch item").await?;
        let (name, size) = match msg {
            Message::Dir { .. } => {
                match recv_required_message(conn, "file after directory marker").await? {
                    Message::File { name, size } => (name, size),
                    other => {
                        return Err(format!(
                            "Expected file after directory marker, received {}",
                            message_kind(&other)
                        ));
                    }
                }
            }
            Message::File { name, size } => (name, size),
            Message::Done => {
                return Err(format!(
                    "Batch ended early after {} of {count} files",
                    files.len()
                ));
            }
            other => {
                return Err(format!(
                    "Unexpected {} message in batch at item {}",
                    message_kind(&other),
                    file_index + 1
                ));
            }
        };

        let path = receive_file(conn, &name, size, out_folder, sort_by_date, handle).await?;
        files.push((name, path, size));

        if let Some(h) = handle {
            let _ = h.emit(
                "lan_transfer_progress",
                serde_json::json!({
                    "direction": "receive",
                    "phase": "transferring",
                    "total_files": count,
                    "received_files": files.len(),
                }),
            );
        }
    }

    match recv_required_message(conn, "batch completion").await? {
        Message::Done => {}
        other => {
            return Err(format!(
                "Expected batch completion, received {}",
                message_kind(&other)
            ));
        }
    }

    if let Some(h) = handle {
        let _ = h.emit(
            "lan_transfer_progress",
            serde_json::json!({
                "direction": "receive",
                "phase": "done",
                "total_files": count,
                "received_files": files.len(),
            }),
        );
    }

    Ok(files)
}

// ─── Utility functions ───

async fn recv_required_message(conn: &Connection, context: &str) -> Result<Message, String> {
    conn.recv_message()
        .await?
        .ok_or_else(|| format!("Peer closed before sending {context}"))
}

async fn ensure_receive_directory(base_dir: &Path, parent: &Path) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(base_dir).await.map_err(|e| {
        format!(
            "Failed to create receive directory {}: {e}",
            base_dir.display()
        )
    })?;
    let canonical_base = tokio::fs::canonicalize(base_dir).await.map_err(|e| {
        format!(
            "Failed to resolve receive directory {}: {e}",
            base_dir.display()
        )
    })?;

    // Check the nearest existing ancestor before creating missing nested
    // directories. This rejects a pre-existing symlink/junction that escapes the
    // selected receive root before create_dir_all can follow it.
    let mut existing_ancestor = parent.to_path_buf();
    loop {
        match tokio::fs::symlink_metadata(&existing_ancestor).await {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !existing_ancestor.pop() {
                    return Err("Receive path has no existing ancestor".into());
                }
            }
            Err(error) => {
                return Err(format!(
                    "Failed to inspect receive directory {}: {error}",
                    existing_ancestor.display()
                ));
            }
        }
    }

    let canonical_ancestor = tokio::fs::canonicalize(&existing_ancestor)
        .await
        .map_err(|e| {
            format!(
                "Failed to resolve receive directory {}: {e}",
                existing_ancestor.display()
            )
        })?;
    if !canonical_ancestor.starts_with(&canonical_base) {
        return Err("Receive directory escapes the configured root".into());
    }

    tokio::fs::create_dir_all(parent).await.map_err(|e| {
        format!(
            "Failed to create receive directory {}: {e}",
            parent.display()
        )
    })?;
    let canonical_parent = tokio::fs::canonicalize(parent).await.map_err(|e| {
        format!(
            "Failed to resolve receive directory {}: {e}",
            parent.display()
        )
    })?;
    if !canonical_parent.starts_with(&canonical_base) {
        return Err("Receive directory escapes the configured root".into());
    }

    Ok(canonical_base)
}

fn message_kind(message: &Message) -> &'static str {
    match message {
        Message::Text { .. } => "text",
        Message::File { .. } => "file",
        Message::Dir { .. } => "directory",
        Message::Batch { .. } => "batch",
        Message::Done => "done",
    }
}

async fn create_partial_file(parent: &Path) -> Result<(tokio::fs::File, PathBuf), String> {
    for _ in 0..PART_FILE_CREATE_ATTEMPTS {
        let path = parent.join(format!(".landrop-{}.part", uuid::Uuid::new_v4()));
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "Failed to create partial receive file {}: {error}",
                    path.display()
                ));
            }
        }
    }

    Err("Failed to allocate a unique partial receive file".into())
}

async fn finalize_partial_file(
    partial_path: &Path,
    desired_out_path: &Path,
) -> Result<PathBuf, String> {
    // Serialize final-name selection within this process so simultaneous receives
    // of the same name cannot choose and overwrite the same destination.
    let lock = RECEIVE_FINALIZE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;
    let out_path = deduplicate_path(desired_out_path)?;
    tokio::fs::rename(partial_path, &out_path)
        .await
        .map_err(|e| {
            format!(
                "Failed to finalize received file {}: {e}",
                out_path.display()
            )
        })?;
    Ok(out_path)
}

fn check_disk_space(path: &Path, needed: u64) -> Result<(), String> {
    let required = needed
        .checked_add(DISK_SPACE_RESERVE_BYTES)
        .ok_or_else(|| "File size exceeds supported disk-space accounting".to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let dir = path.parent().unwrap_or(path);
        let dir_str = dir.to_string_lossy().to_string();
        let wide: Vec<u16> = OsStr::new(&dir_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes: u64 = 0;
        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes as *mut u64,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        if result != 0 && free_bytes < required {
            return Err(format!(
                "Not enough disk space. Need {} but only {} available",
                format_size(needed),
                format_size(free_bytes)
            ));
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::ffi::CString;
        let dir = path.parent().unwrap_or(path);
        let dir_str = dir.to_string_lossy().to_string();
        if let Ok(c_path) = CString::new(dir_str) {
            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
            let result = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
            if result == 0 {
                let free_bytes = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
                if free_bytes < required {
                    return Err(format!(
                        "Not enough disk space. Need {} but only {} available",
                        format_size(needed),
                        format_size(free_bytes)
                    ));
                }
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = (path, needed, required);
    }

    Ok(())
}

/// Windows-style deduplication: file.txt → file (1).txt → file (2).txt.
fn deduplicate_path(path: &Path) -> Result<PathBuf, String> {
    if !path
        .try_exists()
        .map_err(|e| format!("Failed to inspect receive path {}: {e}", path.display()))?
    {
        return Ok(path.to_path_buf());
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 1..=9999 {
        let new_name = match ext {
            Some(e) => format!("{} ({}).{}", stem, i, e),
            None => format!("{} ({})", stem, i),
        };
        let candidate = parent.join(&new_name);
        if !candidate.try_exists().map_err(|e| {
            format!(
                "Failed to inspect receive path {}: {e}",
                candidate.display()
            )
        })? {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Could not choose a unique destination for {}",
        path.display()
    ))
}

fn dirs_next_downloads() -> String {
    // On Android, directories crate doesn't work — use the standard shared Downloads path
    #[cfg(target_os = "android")]
    {
        let android_dl = "/storage/emulated/0/Download/LanDrop";
        let _ = std::fs::create_dir_all(android_dl);
        return android_dl.to_string();
    }

    #[cfg(not(target_os = "android"))]
    {
        if let Some(user_dirs) = directories::UserDirs::new() {
            if let Some(downloads) = user_dirs.download_dir() {
                return downloads.to_string_lossy().to_string();
            }
            // Linux fallback: ~/Downloads if XDG_DOWNLOAD_DIR isn't set
            // (xdg-user-dirs isn't installed on minimal distros like Arch)
            let fallback = user_dirs.home_dir().join("Downloads");
            let _ = std::fs::create_dir_all(&fallback);
            return fallback.to_string_lossy().to_string();
        }
        // Last resort: HOME env var (current_dir is read-only inside AppImage mount)
        if let Ok(home) = std::env::var("HOME") {
            let fallback = std::path::PathBuf::from(home).join("Downloads");
            let _ = std::fs::create_dir_all(&fallback);
            return fallback.to_string_lossy().to_string();
        }
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    }
}

fn resolve_receive_base_dir(out_folder: &str, sort_by_date: bool) -> PathBuf {
    let mut base_dir = if out_folder.is_empty() {
        PathBuf::from(dirs_next_downloads())
    } else {
        PathBuf::from(out_folder)
    };

    if sort_by_date {
        let date_folder = chrono::Local::now().format("%d.%m.%Y").to_string();
        base_dir = base_dir.join(date_folder);
    }

    base_dir
}

#[cfg(test)]
mod tests {
    use super::{
        receive_file, validate_message, Connection, Message, MAX_BATCH_FILES, MAX_TEXT_BYTES,
    };
    use std::net::Ipv4Addr;
    use std::path::{Path, PathBuf};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind loopback listener");
        let address = listener.local_addr().expect("read listener address");
        let (client, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
        (
            client.expect("connect loopback client"),
            accepted.expect("accept loopback client").0,
        )
    }

    fn test_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("landrop-transfer-{label}-{}", uuid::Uuid::new_v4()))
    }

    fn assert_no_partial_files(directory: &Path) {
        let has_partial = std::fs::read_dir(directory)
            .expect("read test directory")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".part"));
        assert!(!has_partial, "partial receive file was not cleaned up");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn outgoing_handshake_rejects_unexpected_peer_uuid() {
        let (client, mut server) = tcp_pair().await;
        let my_uuid = [1u8; 16];
        let expected_peer_uuid = [2u8; 16];
        let unexpected_peer_uuid = [3u8; 16];

        let server_handshake = async {
            let mut received_uuid = [0u8; 16];
            server
                .read_exact(&mut received_uuid)
                .await
                .expect("read client identity");
            assert_eq!(received_uuid, my_uuid);
            server
                .write_all(&unexpected_peer_uuid)
                .await
                .expect("send server identity");
        };

        let (connection, ()) = tokio::join!(
            Connection::from_outgoing(client, &my_uuid, &expected_peer_uuid),
            server_handshake
        );
        let error = connection.err().expect("identity mismatch should fail");
        assert!(error.contains("Peer identity mismatch"), "{error}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn truncated_control_payload_is_not_clean_eof() {
        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let truncated_write = async {
            client
                .write_all(&10u32.to_be_bytes())
                .await
                .expect("write frame length");
            client
                .write_all(b"{}")
                .await
                .expect("write partial payload");
            client.shutdown().await.expect("close client stream");
        };

        let (message, ()) = tokio::join!(connection.recv_message(), truncated_write);
        let error = message.expect_err("truncated payload should fail");
        assert!(error.contains("Truncated control frame payload"), "{error}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn completed_receive_is_renamed_without_overwriting() {
        let directory = test_directory("complete");
        std::fs::create_dir_all(&directory).expect("create test directory");
        std::fs::write(directory.join("script.sh"), b"existing").expect("create existing file");

        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);
        let payload = b"received content";
        let folder = directory.to_string_lossy().into_owned();

        let send = async {
            client.write_all(payload).await.expect("write file payload");
            client.shutdown().await.expect("close client stream");
        };
        let receive = receive_file(
            &connection,
            "script.sh",
            payload.len() as u64,
            &folder,
            false,
            None,
        );

        let (received_path, ()) = tokio::join!(receive, send);
        let received_path = PathBuf::from(received_path.expect("receive complete file"));
        assert_eq!(
            received_path.file_name().and_then(|name| name.to_str()),
            Some("script (1).sh")
        );
        assert_eq!(
            std::fs::read(directory.join("script.sh")).expect("read original file"),
            b"existing"
        );
        assert_eq!(
            std::fs::read(&received_path).expect("read received file"),
            payload
        );
        assert_no_partial_files(&directory);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&received_path)
                .expect("read received permissions")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0, "received script became executable");
        }

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn interrupted_receive_removes_partial_file() {
        let directory = test_directory("interrupted");
        std::fs::create_dir_all(&directory).expect("create test directory");

        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);
        let payload = b"short";
        let folder = directory.to_string_lossy().into_owned();

        let send = async {
            client
                .write_all(payload)
                .await
                .expect("write partial payload");
            client.shutdown().await.expect("close client stream");
        };
        let receive = receive_file(
            &connection,
            "incomplete.bin",
            (payload.len() + 10) as u64,
            &folder,
            false,
            None,
        );

        let (received_path, ()) = tokio::join!(receive, send);
        let error = received_path.expect_err("interrupted receive should fail");
        assert!(error.contains("before the file was complete"), "{error}");
        assert!(!directory.join("incomplete.bin").exists());
        assert_no_partial_files(&directory);

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn symlinked_receive_parent_cannot_escape_root() {
        use std::os::unix::fs::symlink;

        let directory = test_directory("symlink-root");
        let outside = test_directory("symlink-outside");
        std::fs::create_dir_all(&directory).expect("create receive root");
        std::fs::create_dir_all(&outside).expect("create outside directory");
        symlink(&outside, directory.join("escape")).expect("create escaping symlink");

        let (_client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);
        let folder = directory.to_string_lossy().into_owned();
        let result = receive_file(
            &connection,
            "escape/nested/file.bin",
            0,
            &folder,
            false,
            None,
        )
        .await;

        let error = result.expect_err("escaping symlink should be rejected");
        assert!(error.contains("escapes the configured root"), "{error}");
        assert!(
            !outside.join("nested").exists(),
            "receive created directories outside the configured root"
        );

        std::fs::remove_dir_all(directory).expect("remove receive root");
        std::fs::remove_dir_all(outside).expect("remove outside directory");
    }

    #[test]
    fn rejects_excessive_control_values() {
        assert!(validate_message(&Message::Batch {
            count: MAX_BATCH_FILES + 1
        })
        .is_err());
        assert!(validate_message(&Message::Text {
            text: "x".repeat(MAX_TEXT_BYTES + 1)
        })
        .is_err());
    }
}
