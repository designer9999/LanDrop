use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

        // Read sender's UUID
        let mut peer_uuid = [0u8; 16];
        stream
            .read_exact(&mut peer_uuid)
            .await
            .map_err(|e| e.to_string())?;
        // Send our UUID back as ACK
        stream.write_all(my_uuid).await.map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;

        let sender_id = uuid::Uuid::from_bytes(peer_uuid).to_string();
        Ok((Arc::new(Self::from_stream(stream)), sender_id))
    }

    /// Connect to a peer: send our UUID, read their UUID back.
    pub async fn from_outgoing(
        mut stream: TcpStream,
        my_uuid: &[u8; 16],
    ) -> Result<Arc<Self>, String> {
        stream.set_nodelay(true).map_err(|e| e.to_string())?;

        // Send our UUID
        stream.write_all(my_uuid).await.map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;
        // Read peer's UUID (ACK)
        let mut _peer_uuid = [0u8; 16];
        stream
            .read_exact(&mut _peer_uuid)
            .await
            .map_err(|e| e.to_string())?;

        Ok(Arc::new(Self::from_stream(stream)))
    }

    pub async fn send_message(&self, msg: &Message) -> Result<(), String> {
        let json = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
        let len = (json.len() as u32).to_be_bytes();
        let mut w = self.writer.lock().await;
        w.write_all(&len).await.map_err(|e| e.to_string())?;
        w.write_all(&json).await.map_err(|e| e.to_string())?;
        w.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn recv_message(&self) -> Result<Message, String> {
        let mut r = self.reader.lock().await;
        let mut len_buf = [0u8; 4];
        r.read_exact(&mut len_buf)
            .await
            .map_err(|e| e.to_string())?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > 10_000_000 {
            return Err("Message too large".into());
        }

        let mut buf = vec![0u8; len];
        r.read_exact(&mut buf).await.map_err(|e| e.to_string())?;
        serde_json::from_slice(&buf).map_err(|e| e.to_string())
    }

    pub async fn recv_raw(&self, buf: &mut [u8]) -> Result<(), String> {
        let mut r = self.reader.lock().await;
        r.read_exact(buf).await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Write a framed message directly to a writer (caller already holds the lock)
async fn write_message(
    w: &mut tokio::net::tcp::OwnedWriteHalf,
    msg: &Message,
) -> Result<(), String> {
    let json = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
    let len = (json.len() as u32).to_be_bytes();
    w.write_all(&len).await.map_err(|e| e.to_string())?;
    w.write_all(&json).await.map_err(|e| e.to_string())?;
    Ok(())
}

// ─── On-demand send functions ───

/// Open a TCP connection to peer, authenticate, send text, close.
pub async fn send_text_to_peer(
    peer_ip: &str,
    my_uuid: &[u8; 16],
    text: &str,
) -> Result<(), String> {
    let addr: SocketAddr = format!("{}:{}", peer_ip, TCP_PORT)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let stream = time::timeout(Duration::from_secs(3), TcpStream::connect(addr))
        .await
        .map_err(|_| format!("Connection timeout to {}", addr))?
        .map_err(|e| format!("Cannot connect to {}: {}", addr, e))?;

    let conn = Connection::from_outgoing(stream, my_uuid).await?;
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

/// Open a TCP connection to peer, authenticate, send files, close.
pub async fn send_files_to_peer(
    peer_ip: &str,
    my_uuid: &[u8; 16],
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

    // Calculate total size for progress
    let total_size: u64 = {
        let mut total = 0u64;
        for (_, fp) in &file_entries {
            if let Ok(m) = tokio::fs::metadata(fp).await {
                total += m.len();
            }
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

    let conn = Connection::from_outgoing(stream, my_uuid).await?;

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
    write_message(
        &mut w,
        &Message::Batch {
            count: file_entries.len() as u32,
        },
    )
    .await?;

    let mut sent_bytes: u64 = 0;
    let mut sent_files: usize = 0;

    for (name, file_path) in &file_entries {
        let metadata = tokio::fs::metadata(file_path)
            .await
            .map_err(|e| e.to_string())?;
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

        let mut file = tokio::fs::File::open(file_path)
            .await
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut remaining = size;

        while remaining > 0 {
            let to_read = std::cmp::min(remaining as usize, CHUNK_SIZE);
            let n = file
                .read(&mut buf[..to_read])
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
            remaining -= n as u64;
            sent_bytes += n as u64;

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

        w.flush().await.map_err(|e| e.to_string())?;
        sent_files += 1;
    }

    // Always send Done so receiver knows the transfer is complete
    write_message(&mut w, &Message::Done).await?;
    w.flush().await.map_err(|e| e.to_string())?;

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

    let out_path = base_dir.join(&safe_name);

    let canonical_base = base_dir.canonicalize().unwrap_or_else(|_| base_dir.clone());
    if let Ok(canonical_out) = out_path.canonicalize() {
        if !canonical_out.starts_with(&canonical_base) {
            return Err("Path traversal detected".into());
        }
    }
    if let Some(parent) = out_path.parent() {
        if parent.exists() {
            let canonical_parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            if !canonical_parent.starts_with(&canonical_base) {
                return Err("Path traversal detected".into());
            }
        }
    }

    check_disk_space(&out_path, size)?;

    if let Some(parent) = out_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Auto-rename to avoid overwriting: file.txt → file (1).txt → file (2).txt
    let out_path = deduplicate_path(out_path);

    let mut file = tokio::fs::File::create(&out_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut remaining = size;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut received_bytes: u64 = 0;

    while remaining > 0 {
        let to_read = std::cmp::min(remaining as usize, CHUNK_SIZE);
        conn.recv_raw(&mut buf[..to_read]).await?;
        file.write_all(&buf[..to_read])
            .await
            .map_err(|e| e.to_string())?;
        remaining -= to_read as u64;
        received_bytes += to_read as u64;

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

    file.flush().await.map_err(|e| e.to_string())?;

    // On Unix, set executable bit on common script/binary types so users can run
    // received scripts. Default tokio File create gives 0644 which strips +x.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exec_exts = [
            "sh", "bash", "zsh", "fish", "py", "pl", "rb", "AppImage", "appimage",
        ];
        let is_executable = out_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| exec_exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
            .unwrap_or(false)
            || out_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case("AppImage"))
                .unwrap_or(false);
        if is_executable {
            if let Ok(meta) = tokio::fs::metadata(&out_path).await {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = tokio::fs::set_permissions(&out_path, perms).await;
            }
        }
    }

    Ok(out_path.to_string_lossy().to_string())
}

pub async fn receive_batch(
    conn: &Connection,
    count: u32,
    out_folder: &str,
    sort_by_date: bool,
    handle: Option<&AppHandle>,
) -> Result<Vec<(String, String, u64)>, String> {
    let mut files = Vec::new();

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

    for _ in 0..count {
        let msg = conn.recv_message().await?;
        match msg {
            Message::Dir { .. } => {
                let file_msg = conn.recv_message().await?;
                if let Message::File { name, size } = file_msg {
                    let path =
                        receive_file(conn, &name, size, out_folder, sort_by_date, handle).await?;
                    files.push((name, path, size));
                }
            }
            Message::File { name, size } => {
                let path =
                    receive_file(conn, &name, size, out_folder, sort_by_date, handle).await?;
                files.push((name, path, size));
            }
            Message::Done => break,
            _ => {}
        }

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

    if files.len() == count as usize {
        let _ = conn.recv_message().await;
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

fn check_disk_space(path: &Path, needed: u64) -> Result<(), String> {
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

        if result != 0 && free_bytes < needed + 10_000_000 {
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
                let free_bytes = stat.f_bavail as u64 * stat.f_frsize as u64;
                if free_bytes < needed + 10_000_000 {
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
        let _ = (path, needed);
    }

    Ok(())
}

/// Windows-style deduplication: file.txt → file (1).txt → file (2).txt
fn deduplicate_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
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
        if !candidate.exists() {
            return candidate;
        }
    }
    path
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
