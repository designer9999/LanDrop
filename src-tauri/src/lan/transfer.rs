use serde::Serialize;
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time;
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

use super::protocol::{parse_caps, ChatEvent, Message, CHUNK_SIZE, TCP_PORT};

use crate::path_utils::sanitize_relative_path;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CONTROL_IO_TIMEOUT: Duration = Duration::from_secs(30);
const FILE_IDLE_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a sender waits after `done` for the receiver's `ack`.
const ACK_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONTROL_FRAME_BYTES: usize = 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_TRANSFER_PATH_BYTES: usize = 4096;
const MAX_BATCH_FILES: u32 = 100_000;
const MAX_MESSAGE_ID_BYTES: usize = 128;
const MAX_CHAT_KIND_BYTES: usize = 32;
const MAX_EMOJI_BYTES: usize = 64;
const MAX_ACK_ERROR_BYTES: usize = 1024;
/// Wire bounds for identity capabilities; stored lists are further trimmed
/// by `parse_caps`.
const MAX_WIRE_CAPS: usize = 64;
const MAX_WIRE_CAP_BYTES: usize = 64;
const DISK_SPACE_RESERVE_BYTES: u64 = 10_000_000;
const PART_FILE_CREATE_ATTEMPTS: usize = 16;
const RECEIVE_WRITE_BUFFER_BYTES: usize = 1024 * 1024;

/// Error string a cancelled send rejects with. The frontend matches it exactly.
pub const CANCELLED: &str = "cancelled";

static RECEIVE_FINALIZE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const PROGRESS_EMIT_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// Rate-limits `lan_transfer_progress` emissions to at most one per 100 ms or
/// per 1% of total bytes — whichever comes first. A 10 GB transfer otherwise
/// serializes 40k+ events into the WebView. Terminal events bypass this.
struct ProgressThrottle {
    last_emit: Option<Instant>,
    last_bytes: u64,
    min_bytes_delta: u64,
}

impl ProgressThrottle {
    fn new(total_bytes: u64) -> Self {
        Self {
            last_emit: None,
            last_bytes: 0,
            min_bytes_delta: (total_bytes / 100).max(1),
        }
    }

    fn should_emit(&mut self, transferred_bytes: u64) -> bool {
        let due_time = self
            .last_emit
            .is_none_or(|at| at.elapsed() >= PROGRESS_EMIT_MIN_INTERVAL);
        let due_bytes = transferred_bytes.saturating_sub(self.last_bytes) >= self.min_bytes_delta;
        if due_time || due_bytes {
            self.last_emit = Some(Instant::now());
            self.last_bytes = transferred_bytes;
            true
        } else {
            false
        }
    }
}

/// Result of a send that completed without error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SendStatus {
    /// The receiver acknowledged committing the unit.
    Delivered,
    /// Written and terminated with `done`, but not acknowledged (1.8.x peer,
    /// no message id, EOF or acknowledgement timeout).
    Sent,
    /// The peer does not support this operation (chat frame to a non-`chat2` peer).
    Unsupported,
}

/// Upper bound for a quoted-message preview on the wire (200 chars of UTF-8).
const MAX_REPLY_PREVIEW_BYTES: usize = 800;

/// Optional v2 identifiers carried by a text or batch unit.
#[derive(Debug, Clone, Default)]
pub struct UnitMeta {
    pub id: Option<String>,
    pub reply_to: Option<String>,
    pub reply_preview: Option<String>,
}

/// Where `lan_transfer_progress` events go, tagged with the message and peer
/// they belong to so the UI can track several transfers at once.
#[derive(Clone, Copy, Default)]
pub struct ProgressSink<'a> {
    pub handle: Option<&'a AppHandle>,
    pub message_id: Option<&'a str>,
    pub peer_id: Option<&'a str>,
}

impl ProgressSink<'_> {
    pub fn emit(&self, mut payload: serde_json::Value) {
        let Some(handle) = self.handle else {
            return;
        };
        if let Some(map) = payload.as_object_mut() {
            if let Some(id) = self.message_id {
                map.insert("message_id".into(), id.into());
            }
            if let Some(id) = self.peer_id {
                map.insert("peer_id".into(), id.into());
            }
        }
        let _ = handle.emit("lan_transfer_progress", payload);
    }
}

/// Whole-unit receive progress. The protocol announces each file's size only
/// when that file starts, so `total_bytes` is the sum of sizes announced so far
/// and grows as a batch proceeds; `total_files` is known from the batch header.
/// Intermediate events are throttled to ~10 Hz per unit, not per file.
pub struct ReceiveProgress<'a> {
    sink: ProgressSink<'a>,
    total_files: u32,
    received_files: u32,
    total_bytes: u64,
    received_bytes: u64,
    last_emit: Option<Instant>,
}

impl<'a> ReceiveProgress<'a> {
    pub fn new(sink: ProgressSink<'a>, total_files: u32) -> Self {
        Self {
            sink,
            total_files,
            received_files: 0,
            total_bytes: 0,
            received_bytes: 0,
            last_emit: None,
        }
    }

    /// Android resolves its private receive directory through the app handle.
    #[cfg(target_os = "android")]
    pub fn handle(&self) -> Option<&'a AppHandle> {
        self.sink.handle
    }

    /// Emit unconditionally (start and terminal phases).
    pub fn emit(&mut self, phase: &str, current_file: Option<&str>) {
        if self.sink.handle.is_none() {
            return;
        }
        let mut payload = serde_json::json!({
            "direction": "receive",
            "phase": phase,
            "total_files": self.total_files,
            "received_files": self.received_files,
            "total_bytes": self.total_bytes,
            "received_bytes": self.received_bytes,
        });
        if let Some(name) = current_file {
            payload["current_file"] = name.into();
        }
        self.sink.emit(payload);
        self.last_emit = Some(Instant::now());
    }

    pub fn emit_error(&self, error: &str) {
        self.sink.emit(serde_json::json!({
            "direction": "receive",
            "phase": "error",
            "error": error,
            "total_files": self.total_files,
            "received_files": self.received_files,
            "total_bytes": self.total_bytes,
            "received_bytes": self.received_bytes,
        }));
    }

    fn due(&self) -> bool {
        self.sink.handle.is_some()
            && self
                .last_emit
                .is_none_or(|at| at.elapsed() >= PROGRESS_EMIT_MIN_INTERVAL)
    }

    /// A file header arrived announcing `size` more bytes.
    pub fn begin_file(&mut self, size: u64) {
        self.total_bytes = self.total_bytes.saturating_add(size);
    }

    fn add_bytes(&mut self, bytes: u64, current_file: &str) {
        self.received_bytes = self.received_bytes.saturating_add(bytes);
        if self.due() {
            self.emit("transferring", Some(current_file));
        }
    }

    pub fn finish_file(&mut self, current_file: &str) {
        self.received_files = self.received_files.saturating_add(1);
        if self.due() {
            self.emit("transferring", Some(current_file));
        }
    }
}

/// Run `work` until it finishes or `cancel` fires. Cancelling drops the work
/// future, which closes its connection; the receiver then sees an incomplete
/// transfer and removes its partial file.
pub async fn with_cancel<T>(
    cancel: Option<&CancellationToken>,
    work: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let Some(cancel) = cancel else {
        return work.await;
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(CANCELLED.into()),
        result = work => result,
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

pub struct Connection {
    reader: Mutex<OwnedReadHalf>,
    writer: Mutex<OwnedWriteHalf>,
    #[cfg(windows)]
    tailnet_route: Option<(
        std::net::Ipv4Addr,
        Option<super::windows_tailnet::RouteBinding>,
    )>,
}

impl Connection {
    fn from_stream(stream: TcpStream) -> Self {
        #[cfg(windows)]
        let tailnet_route = stream
            .peer_addr()
            .ok()
            .and_then(|address| match address.ip() {
                std::net::IpAddr::V4(ip) if super::tailscale::is_tailscale_ipv4(ip) => {
                    Some((ip, super::windows_tailnet::binding_for(ip)))
                }
                _ => None,
            });
        let (reader, writer) = stream.into_split();
        Self {
            reader: Mutex::new(reader),
            writer: Mutex::new(writer),
            #[cfg(windows)]
            tailnet_route,
        }
    }

    /// Windows tailnet sockets must never continue under a stale adapter/profile
    /// snapshot. This is route validation, not cryptographic UUID authentication.
    pub(super) fn ensure_current_route(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some((ip, binding)) = self.tailnet_route {
            if binding.is_none() || super::windows_tailnet::binding_for(ip) != binding {
                return Err("Tailscale route changed; reconnect before sending again".into());
            }
        }
        Ok(())
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
        self.ensure_current_route()?;
        write_message(&mut w, msg).await?;
        time::timeout(CONTROL_IO_TIMEOUT, w.flush())
            .await
            .map_err(|_| "Timed out flushing control message".to_string())?
            .map_err(|e| format!("Failed to flush control message: {e}"))?;
        self.ensure_current_route()?;
        Ok(())
    }

    /// Read one framed message.
    ///
    /// `Ok(None)` means the peer closed cleanly before sending any byte of the
    /// next frame. A partial length, partial payload, timeout, invalid JSON, or
    /// invalid message is an error.
    pub async fn recv_message(&self) -> Result<Option<Message>, String> {
        let mut r = self.reader.lock().await;
        self.ensure_current_route()?;
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
        self.ensure_current_route()?;
        Ok(Some(message))
    }

    pub async fn recv_raw(&self, buf: &mut [u8]) -> Result<usize, String> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut r = self.reader.lock().await;
        self.ensure_current_route()?;
        let read = time::timeout(FILE_IDLE_TIMEOUT, r.read(buf))
            .await
            .map_err(|_| "File transfer timed out waiting for data".to_string())?
            .map_err(|e| format!("Failed to receive file data: {e}"))?;
        self.ensure_current_route()?;
        if read == 0 {
            return Err("Peer closed the connection before the file was complete".into());
        }
        Ok(read)
    }
}

fn id_too_long(id: Option<&String>) -> bool {
    id.is_some_and(|id| id.is_empty() || id.len() > MAX_MESSAGE_ID_BYTES)
}

fn validate_message(msg: &Message) -> Result<(), String> {
    match msg {
        Message::Identity {
            app,
            alias,
            device_type,
            caps,
        } if app.len() > 32
            || alias.len() > 256
            || device_type.len() > 32
            || caps.len() > MAX_WIRE_CAPS
            || caps.iter().any(|cap| cap.len() > MAX_WIRE_CAP_BYTES) =>
        {
            Err("Discovery identity fields exceed their size limits".into())
        }
        Message::Text { text, .. } if text.len() > MAX_TEXT_BYTES => Err(format!(
            "Text message too large: {} bytes (maximum {MAX_TEXT_BYTES})",
            text.len()
        )),
        Message::Text { id, reply_to, .. } | Message::Batch { id, reply_to, .. }
            if id_too_long(id.as_ref()) || id_too_long(reply_to.as_ref()) =>
        {
            Err("Message identifier is empty or too long".into())
        }
        Message::Text { reply_preview, .. } | Message::Batch { reply_preview, .. }
            if reply_preview
                .as_ref()
                .is_some_and(|preview| preview.len() > MAX_REPLY_PREVIEW_BYTES) =>
        {
            Err("Reply preview is too long".into())
        }
        Message::File { name, .. } | Message::Dir { name }
            if name.len() > MAX_TRANSFER_PATH_BYTES =>
        {
            Err(format!(
                "Transfer path too long: {} bytes (maximum {MAX_TRANSFER_PATH_BYTES})",
                name.len()
            ))
        }
        Message::Batch { count, .. } if *count > MAX_BATCH_FILES => Err(format!(
            "Batch contains too many files: {count} (maximum {MAX_BATCH_FILES})"
        )),
        Message::Ack { id, error, .. }
            if id_too_long(Some(id))
                || error
                    .as_ref()
                    .is_some_and(|e| e.len() > MAX_ACK_ERROR_BYTES) =>
        {
            Err("Acknowledgement fields exceed their size limits".into())
        }
        Message::Chat {
            kind,
            id,
            text,
            emoji,
        } if kind.len() > MAX_CHAT_KIND_BYTES
            || id_too_long(Some(id))
            || text.as_ref().is_some_and(|t| t.len() > MAX_TEXT_BYTES)
            || emoji.as_ref().is_some_and(|e| e.len() > MAX_EMOJI_BYTES) =>
        {
            Err("Chat event fields exceed their size limits".into())
        }
        _ => Ok(()),
    }
}

/// Encode one control frame (`u32` big-endian length + JSON) into a single
/// buffer so it leaves in one write rather than two small segments.
fn encode_frame(msg: &Message) -> Result<Vec<u8>, String> {
    validate_message(msg)?;
    let mut frame = vec![0u8; 4];
    serde_json::to_writer(&mut frame, msg).map_err(|e| format!("Failed to encode message: {e}"))?;
    let json_len = frame.len() - 4;
    if json_len > MAX_CONTROL_FRAME_BYTES {
        return Err(format!(
            "Control message too large: {json_len} bytes (maximum {MAX_CONTROL_FRAME_BYTES})"
        ));
    }
    let len = u32::try_from(json_len)
        .map_err(|_| "Control message length exceeds protocol capacity".to_string())?
        .to_be_bytes();
    frame[..4].copy_from_slice(&len);
    Ok(frame)
}

/// Write a framed message directly to a writer (caller already holds the lock)
async fn write_message(
    w: &mut tokio::net::tcp::OwnedWriteHalf,
    msg: &Message,
) -> Result<(), String> {
    let frame = encode_frame(msg)?;
    time::timeout(CONTROL_IO_TIMEOUT, w.write_all(&frame))
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

/// Bind Windows tailnet traffic to the verified adapter's own source address.
/// No system route, VPN setting, or Tailscale control API is changed here.
async fn connect_stream(addr: SocketAddr) -> Result<TcpStream, String> {
    #[cfg(windows)]
    if let std::net::IpAddr::V4(ip) = addr.ip() {
        if super::tailscale::is_tailscale_ipv4(ip) {
            let binding = super::windows_tailnet::binding_for(ip)
                .ok_or("No current Tailscale host route for this device")?;
            let socket = tokio::net::TcpSocket::new_v4().map_err(|e| e.to_string())?;
            socket
                .bind(SocketAddr::from((binding.local_ipv4, 0)))
                .map_err(|_| "Cannot bind the current Tailscale adapter")?;
            let stream = socket.connect(addr).await.map_err(|e| e.to_string())?;
            if super::windows_tailnet::binding_for(ip) != Some(binding) {
                return Err("Tailscale route changed during connection".into());
            }
            return Ok(stream);
        }
    }
    TcpStream::connect(addr).await.map_err(|e| e.to_string())
}

/// Establish and verify a route before any transfer payload is sent.
pub async fn connect_to_peer(
    peer_ip: &str,
    my_uuid: &[u8; 16],
    expected_peer_uuid: &[u8; 16],
) -> Result<Arc<Connection>, String> {
    let addr: SocketAddr = format!("{}:{}", peer_ip, TCP_PORT)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    connect_at(addr, my_uuid, expected_peer_uuid).await
}

async fn connect_at(
    addr: SocketAddr,
    my_uuid: &[u8; 16],
    expected_peer_uuid: &[u8; 16],
) -> Result<Arc<Connection>, String> {
    time::timeout(Duration::from_secs(3), async {
        let stream = connect_stream(addr)
            .await
            .map_err(|e| format!("Cannot connect to {addr}: {e}"))?;
        Connection::from_outgoing(stream, my_uuid, expected_peer_uuid).await
    })
    .await
    .map_err(|_| format!("Connection timeout to {addr}"))?
}

pub async fn send_text_on_connection(
    conn: &Connection,
    text: &str,
    meta: &UnitMeta,
) -> Result<SendStatus, String> {
    conn.send_message(&Message::Text {
        text: text.to_string(),
        id: meta.id.clone(),
        reply_to: meta.reply_to.clone(),
        reply_preview: meta.reply_preview.clone(),
        // Only v2 sends (with an id) carry a timestamp, so id-less sends keep
        // the exact 1.8.x frame.
        sent_at: meta.id.as_ref().map(|_| now_unix_ms()),
    })
    .await?;
    conn.send_message(&Message::Done).await?;
    match &meta.id {
        Some(id) => await_ack(conn, id).await,
        None => Ok(SendStatus::Sent),
    }
}

/// Send one chat event (edit/delete/react). Callers must have checked that
/// the peer advertises `chat2`.
pub async fn send_chat_on_connection(
    conn: &Connection,
    event: ChatEvent,
) -> Result<SendStatus, String> {
    let id = event.id().to_string();
    conn.send_message(&event.into_message()).await?;
    conn.send_message(&Message::Done).await?;
    await_ack(conn, &id).await
}

/// After `done`, wait briefly for the receiver's acknowledgement of `id`.
/// LanDrop 1.8.x receivers close right after `done`, so EOF returns at once.
async fn await_ack(conn: &Connection, id: &str) -> Result<SendStatus, String> {
    match time::timeout(ACK_TIMEOUT, conn.recv_message()).await {
        Ok(Ok(Some(Message::Ack {
            id: acked,
            ok,
            error,
        }))) if acked == id => {
            if ok {
                Ok(SendStatus::Delivered)
            } else {
                Err(error
                    .filter(|error| !error.trim().is_empty())
                    .unwrap_or_else(|| "The receiver rejected the message".into()))
            }
        }
        // EOF, timeout, a reset after `done`, or an unrelated frame: the unit
        // was written in full but its commit cannot be confirmed.
        _ => Ok(SendStatus::Sent),
    }
}

/// Receiver side: acknowledge a committed (or failed) unit that carried an id.
pub async fn send_ack(conn: &Connection, id: &str, result: Result<(), &str>) -> Result<(), String> {
    let (ok, error) = match result {
        Ok(()) => (true, None),
        Err(error) => (false, Some(truncate_utf8(error, MAX_ACK_ERROR_BYTES))),
    };
    conn.send_message(&Message::Ack {
        id: id.to_string(),
        ok,
        error,
    })
    .await
}

/// Open the LanDrop TCP handshake only and return the peer UUID.
pub async fn probe_peer_id(peer_ip: &str, my_uuid: &[u8; 16]) -> Result<String, String> {
    let addr: SocketAddr = format!("{}:{}", peer_ip, TCP_PORT)
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    time::timeout(Duration::from_millis(700), async {
        let mut stream = connect_stream(addr)
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

/// Application identity returned by a discovery probe.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: String,
    pub alias: String,
    pub device_type: String,
    pub caps: Vec<String>,
}

/// Confirm that a tailnet node actually runs LanDrop, including its application
/// response rather than treating arbitrary UUID-shaped bytes as discovery.
pub async fn probe_peer_info(peer_ip: &str, my_uuid: &[u8; 16]) -> Result<PeerInfo, String> {
    let addr: SocketAddr = format!("{peer_ip}:{TCP_PORT}")
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;
    probe_peer_info_at(addr, my_uuid).await
}

async fn probe_peer_info_at(addr: SocketAddr, my_uuid: &[u8; 16]) -> Result<PeerInfo, String> {
    time::timeout(Duration::from_secs(3), async {
        let mut stream = connect_stream(addr).await?;
        stream.write_all(my_uuid).await.map_err(|e| e.to_string())?;
        let mut remote = [0; 16];
        stream
            .read_exact(&mut remote)
            .await
            .map_err(|e| e.to_string())?;
        let conn = Connection::from_stream(stream);
        conn.send_message(&Message::Discover).await?;
        match conn.recv_message().await? {
            Some(Message::Identity {
                app,
                alias,
                device_type,
                caps,
            }) if app == "LanDrop" => Ok(PeerInfo {
                id: uuid::Uuid::from_bytes(remote).to_string(),
                alias: super::identity::sanitize_alias(&alias),
                device_type,
                caps: parse_caps(&caps.join(",")),
            }),
            _ => Err("Peer did not identify as LanDrop".into()),
        }
    })
    .await
    .map_err(|_| format!("LanDrop discovery timed out at {addr}"))?
}

/// Resolve the files to send (folders walked, symlinks skipped) and their
/// combined size. Synchronous filesystem work: run on the blocking pool.
fn collect_send_entries(paths: &[String]) -> Result<(Vec<(String, PathBuf)>, u64), String> {
    let mut file_entries: Vec<(String, PathBuf)> = Vec::new();

    for path_str in paths {
        let path = Path::new(path_str);

        if path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
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
            return Err(format!("File not accessible: {}", path.display()));
        }
    }

    if file_entries.is_empty() {
        return Err("No files to send".into());
    }

    let mut total = 0u64;
    for (_, fp) in &file_entries {
        let metadata = std::fs::metadata(fp)
            .map_err(|e| format!("Failed to read metadata for {}: {e}", fp.display()))?;
        total = total
            .checked_add(metadata.len())
            .ok_or_else(|| "Combined transfer size exceeds protocol capacity".to_string())?;
    }
    Ok((file_entries, total))
}

/// Prepare files before opening a verified connection, then send exactly once.
pub async fn send_files_on_connection<F>(
    paths: &[String],
    meta: &UnitMeta,
    sink: ProgressSink<'_>,
    connect: F,
) -> Result<SendStatus, String>
where
    F: Future<Output = Result<Arc<Connection>, String>>,
{
    let owned_paths = paths.to_vec();
    let collected = tokio::task::spawn_blocking(move || collect_send_entries(&owned_paths))
        .await
        .map_err(|e| format!("Failed to prepare files: {e}"))?;
    let (file_entries, total_size) = match collected {
        Ok(collected) => collected,
        Err(error) => {
            if let Some(h) = sink.handle {
                let _ = h.emit(
                    "lan_log",
                    serde_json::json!({
                        "level": "error",
                        "text": error,
                    }),
                );
            }
            return Err(error);
        }
    };

    let batch_count = u32::try_from(file_entries.len())
        .map_err(|_| "Too many files for one transfer".to_string())?;
    if batch_count > MAX_BATCH_FILES {
        return Err(format!(
            "Batch contains too many files: {batch_count} (maximum {MAX_BATCH_FILES})"
        ));
    }
    let total_files = file_entries.len();

    let conn = connect.await?;

    sink.emit(serde_json::json!({
        "direction": "send",
        "phase": "start",
        "total_bytes": total_size,
        "total_files": total_files,
        "sent_bytes": 0,
        "sent_files": 0,
    }));

    // Use writer lock for atomic sends (no interleaving with concurrent operations)
    let mut w = conn.writer.lock().await;
    conn.ensure_current_route()?;

    // Always send batch header so receiver knows what to expect
    write_message(
        &mut w,
        &Message::Batch {
            count: batch_count,
            id: meta.id.clone(),
            reply_to: meta.reply_to.clone(),
            reply_preview: meta.reply_preview.clone(),
        },
    )
    .await?;

    let mut sent_bytes: u64 = 0;
    let mut sent_files: usize = 0;
    let mut progress_throttle = ProgressThrottle::new(total_size);
    let mut buf = vec![0u8; CHUNK_SIZE];

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
        conn.ensure_current_route()?;
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
            conn.ensure_current_route()?;
            write_file_chunk(&mut w, &buf[..n]).await?;
            conn.ensure_current_route()?;
            remaining -= n as u64;
            sent_bytes = sent_bytes
                .checked_add(n as u64)
                .ok_or_else(|| "Sent byte counter overflowed".to_string())?;

            if sink.handle.is_some()
                && (sent_bytes == total_size || progress_throttle.should_emit(sent_bytes))
            {
                sink.emit(serde_json::json!({
                    "direction": "send",
                    "phase": "transferring",
                    "total_bytes": total_size,
                    "total_files": total_files,
                    "sent_bytes": sent_bytes,
                    "sent_files": sent_files,
                    "current_file": name,
                }));
            }
        }

        time::timeout(FILE_IDLE_TIMEOUT, w.flush())
            .await
            .map_err(|_| "File transfer timed out while flushing data".to_string())?
            .map_err(|e| format!("Failed to flush file data: {e}"))?;
        sent_files += 1;
    }

    // Always send Done so receiver knows the transfer is complete
    conn.ensure_current_route()?;
    write_message(&mut w, &Message::Done).await?;
    time::timeout(CONTROL_IO_TIMEOUT, w.flush())
        .await
        .map_err(|_| "Timed out flushing transfer completion".to_string())?
        .map_err(|e| format!("Failed to flush transfer completion: {e}"))?;

    drop(w);

    let status = match &meta.id {
        Some(id) => await_ack(&conn, id).await?,
        None => SendStatus::Sent,
    };

    sink.emit(serde_json::json!({
        "direction": "send",
        "phase": "done",
        "total_bytes": total_size,
        "total_files": total_files,
        "sent_bytes": sent_bytes,
        "sent_files": sent_files,
        "status": status,
    }));

    Ok(status)
}

// ─── Receive functions (called by incoming TCP handler) ───

pub async fn receive_file(
    conn: &Connection,
    name: &str,
    size: u64,
    out_folder: &str,
    sort_by_date: bool,
    mut progress: Option<&mut ReceiveProgress<'_>>,
) -> Result<String, String> {
    let safe_name = sanitize_relative_path(name);
    #[cfg(target_os = "android")]
    let default_dir = if out_folder.is_empty() {
        use tauri::Manager;
        Some(
            progress
                .as_ref()
                .and_then(|progress| progress.handle())
                .ok_or("Android receive storage is unavailable")?
                .path()
                .app_data_dir()
                .map_err(|_| "Android receive storage is unavailable")?
                .join("files")
                .join("received"),
        )
    } else {
        None
    };
    #[cfg(not(target_os = "android"))]
    let default_dir: Option<PathBuf> = None;
    let base_dir = resolve_receive_base_dir(out_folder, sort_by_date, default_dir.as_deref())?;
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

    let (file, partial_path) = create_partial_file(parent).await?;
    // Network reads arrive in small pieces; batch them into large disk writes.
    let mut file = tokio::io::BufWriter::with_capacity(RECEIVE_WRITE_BUFFER_BYTES, file);
    let mut remaining = size;
    let mut buf = vec![0u8; CHUNK_SIZE];

    let receive_result: Result<(), String> = async {
        while remaining > 0 {
            let to_read = remaining.min(CHUNK_SIZE as u64) as usize;
            let read = conn.recv_raw(&mut buf[..to_read]).await?;
            file.write_all(&buf[..read])
                .await
                .map_err(|e| format!("Failed to write received file: {e}"))?;
            remaining -= read as u64;
            if let Some(progress) = progress.as_deref_mut() {
                progress.add_bytes(read as u64, name);
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

#[cfg(test)]
pub async fn receive_batch(
    conn: &Connection,
    count: u32,
    out_folder: &str,
    sort_by_date: bool,
) -> Result<Vec<(String, String, u64)>, String> {
    let mut files = Vec::new();
    let mut progress = ReceiveProgress::new(ProgressSink::default(), count);
    receive_batch_into(
        conn,
        count,
        out_folder,
        sort_by_date,
        &mut progress,
        &mut files,
    )
    .await?;
    Ok(files)
}

/// Receive a batch, pushing each finalized file into `files` as soon as it is on
/// disk. On error, `files` still lists everything already written so the caller
/// can surface it instead of leaving untracked files in the receive folder.
pub async fn receive_batch_into(
    conn: &Connection,
    count: u32,
    out_folder: &str,
    sort_by_date: bool,
    progress: &mut ReceiveProgress<'_>,
    files: &mut Vec<(String, String, u64)>,
) -> Result<(), String> {
    // Emit before validation: the caller emits a terminal error even for an
    // invalid batch. Every terminal event must have its own matching start,
    // otherwise it can incorrectly decrement a different active receive.
    progress.emit("start", None);

    if count > MAX_BATCH_FILES {
        return Err(format!(
            "Batch contains too many files: {count} (maximum {MAX_BATCH_FILES})"
        ));
    }
    files.reserve(count as usize);

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

        progress.begin_file(size);
        let path = receive_file(
            conn,
            &name,
            size,
            out_folder,
            sort_by_date,
            Some(&mut *progress),
        )
        .await?;
        progress.finish_file(&name);
        files.push((name, path, size));
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

    progress.emit("done", None);

    Ok(())
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
        Message::Discover => "discovery",
        Message::Identity { .. } => "identity",
        Message::Ack { .. } => "acknowledgement",
        Message::Chat { .. } => "chat",
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
        use std::os::windows::ffi::OsStrExt;

        let dir = path.parent().unwrap_or(path);
        let wide: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes: u64 = 0;
        #[expect(unsafe_code, reason = "disk-space query requires the Win32 FFI call")]
        let result = unsafe {
            windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                windows::core::PCWSTR(wide.as_ptr()),
                Some(&mut free_bytes),
                None,
                None,
            )
        };

        if result.is_ok() && free_bytes < required {
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
            #[expect(unsafe_code, reason = "statvfs requires a zeroed out-parameter")]
            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
            #[expect(unsafe_code, reason = "disk-space query requires the libc FFI call")]
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

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(not(target_os = "android"))]
fn dirs_next_downloads() -> String {
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

fn resolve_receive_base_dir(
    out_folder: &str,
    sort_by_date: bool,
    default_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    let mut base_dir = if out_folder.is_empty() {
        if let Some(directory) = default_dir {
            directory.to_owned()
        } else {
            #[cfg(target_os = "android")]
            {
                return Err("Android receive storage is unavailable".into());
            }
            #[cfg(not(target_os = "android"))]
            {
                PathBuf::from(dirs_next_downloads())
            }
        }
    } else {
        PathBuf::from(out_folder)
    };

    if sort_by_date {
        let date_folder = chrono::Local::now().format("%d.%m.%Y").to_string();
        base_dir = base_dir.join(date_folder);
    }

    Ok(base_dir)
}

#[cfg(test)]
mod tests {
    use super::{
        receive_batch, receive_file, validate_message, Connection, Message, MAX_BATCH_FILES,
        MAX_CONTROL_FRAME_BYTES, MAX_TEXT_BYTES,
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

    #[tokio::test(flavor = "current_thread")]
    #[cfg(windows)]
    async fn tailnet_session_without_current_route_rejects_io() {
        let (stream, _peer) = tcp_pair().await;
        let mut connection = Connection::from_stream(stream);
        connection.tailnet_route = Some((Ipv4Addr::new(100, 64, 1, 1), None));
        assert!(connection.send_message(&Message::Done).await.is_err());
        assert!(connection.recv_message().await.is_err());
        assert!(connection.recv_raw(&mut [0; 8]).await.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn discovery_requires_landrop_application_response() {
        for app in ["LanDrop", "OtherService"] {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let address = listener.local_addr().unwrap();
            let remote_uuid = [2; 16];
            let server = async {
                let (stream, _) = listener.accept().await.unwrap();
                let (connection, _) = Connection::from_incoming(stream, &remote_uuid)
                    .await
                    .unwrap();
                assert!(matches!(
                    connection.recv_message().await.unwrap(),
                    Some(Message::Discover)
                ));
                connection
                    .send_message(&Message::Identity {
                        app: app.into(),
                        alias: "Colleague".into(),
                        device_type: "desktop".into(),
                        caps: vec!["ack".into(), "chat2".into()],
                    })
                    .await
                    .unwrap();
            };
            let (result, ()) = tokio::join!(super::probe_peer_info_at(address, &[1; 16]), server);
            if app == "LanDrop" {
                let info = result.unwrap();
                assert_eq!(info.id, uuid::Uuid::from_bytes(remote_uuid).to_string());
                assert_eq!(info.alias, "Colleague");
                assert_eq!(info.device_type, "desktop");
                assert_eq!(info.caps, vec!["ack", "chat2"]);
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn discovery_rejects_uuid_only_services() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut sender = [0; 16];
            stream.read_exact(&mut sender).await.unwrap();
            stream.write_all(&[2; 16]).await.unwrap();
        };
        let (result, ()) = tokio::join!(super::probe_peer_info_at(address, &[1; 16]), server);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn verified_connection_is_reused_for_text_payload() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            assert!(
                matches!(connection.recv_message().await.unwrap(), Some(Message::Text { text, id: None, .. }) if text == "hello over the selected route")
            );
            assert!(matches!(
                connection.recv_message().await.unwrap(),
                Some(Message::Done)
            ));
        };
        let client = async {
            let connection = super::connect_at(address, &[1; 16], &[2; 16])
                .await
                .unwrap();
            let status = super::send_text_on_connection(
                &connection,
                "hello over the selected route",
                &super::UnitMeta::default(),
            )
            .await
            .unwrap();
            assert_eq!(status, super::SendStatus::Sent);
        };
        tokio::join!(server, client);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn verified_connection_sends_and_receives_a_complete_file_batch() {
        let source = test_directory("route-source");
        let destination = test_directory("route-destination");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let source_file = source.join("hello.txt");
        std::fs::write(&source_file, b"file payload over selected route").unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            let Some(Message::Batch { count, .. }) = connection.recv_message().await.unwrap()
            else {
                panic!("expected file batch");
            };
            let files = receive_batch(&connection, count, destination.to_str().unwrap(), false)
                .await
                .unwrap();
            assert_eq!(files.len(), 1);
            assert_eq!(
                std::fs::read(&files[0].1).unwrap(),
                b"file payload over selected route"
            );
        };
        let client = async {
            super::send_files_on_connection(
                &[source_file.to_string_lossy().into_owned()],
                &super::UnitMeta::default(),
                super::ProgressSink::default(),
                super::connect_at(address, &[1; 16], &[2; 16]),
            )
            .await
            .unwrap();
        };
        tokio::join!(server, client);
        std::fs::remove_dir_all(source).unwrap();
        std::fs::remove_dir_all(destination).unwrap();
    }

    fn v2_meta(id: &str) -> super::UnitMeta {
        super::UnitMeta {
            id: Some(id.into()),
            reply_to: Some("earlier".into()),
            reply_preview: Some("the earlier message".into()),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v2_text_is_acknowledged_as_delivered() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            let Some(Message::Text {
                text,
                id: Some(id),
                reply_to,
                reply_preview,
                sent_at,
            }) = connection.recv_message().await.unwrap()
            else {
                panic!("expected v2 text");
            };
            assert_eq!(text, "hello");
            assert_eq!(reply_to.as_deref(), Some("earlier"));
            assert_eq!(reply_preview.as_deref(), Some("the earlier message"));
            assert!(sent_at.is_some_and(|ms| ms > 1_600_000_000_000));
            super::send_ack(&connection, &id, Ok(())).await.unwrap();
            assert!(matches!(
                connection.recv_message().await.unwrap(),
                Some(Message::Done)
            ));
        };
        let client = async {
            let connection = super::connect_at(address, &[1; 16], &[2; 16])
                .await
                .unwrap();
            super::send_text_on_connection(&connection, "hello", &v2_meta("m-1")).await
        };
        let ((), status) = tokio::join!(server, client);
        assert_eq!(status.unwrap(), super::SendStatus::Delivered);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn legacy_receiver_closing_after_done_returns_sent_without_waiting() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        // Behaves like a 1.8.x receiver: reads the unit, never acks, closes.
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            assert!(matches!(
                connection.recv_message().await.unwrap(),
                Some(Message::Text { .. })
            ));
            assert!(matches!(
                connection.recv_message().await.unwrap(),
                Some(Message::Done)
            ));
        };
        let client = async {
            let connection = super::connect_at(address, &[1; 16], &[2; 16])
                .await
                .unwrap();
            let started = std::time::Instant::now();
            let status =
                super::send_text_on_connection(&connection, "hello", &v2_meta("m-2")).await;
            (status, started.elapsed())
        };
        let ((), (status, elapsed)) = tokio::join!(server, client);
        assert_eq!(status.unwrap(), super::SendStatus::Sent);
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "EOF must not wait for the ack timeout: {elapsed:?}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn negative_ack_rejects_the_send_with_the_receiver_error() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            let _ = connection.recv_message().await.unwrap();
            super::send_ack(&connection, "m-3", Err("Not enough disk space"))
                .await
                .unwrap();
            let _ = connection.recv_message().await;
        };
        let client = async {
            let connection = super::connect_at(address, &[1; 16], &[2; 16])
                .await
                .unwrap();
            super::send_text_on_connection(&connection, "hello", &v2_meta("m-3")).await
        };
        let ((), status) = tokio::join!(server, client);
        assert_eq!(status.unwrap_err(), "Not enough disk space");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn v2_batch_carries_its_id_and_is_acknowledged() {
        let source = test_directory("ack-source");
        let destination = test_directory("ack-destination");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let source_file = source.join("photo.jpg");
        std::fs::write(&source_file, b"not really a jpeg").unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            let Some(Message::Batch {
                count,
                id: Some(id),
                reply_to,
                reply_preview,
            }) = connection.recv_message().await.unwrap()
            else {
                panic!("expected v2 batch");
            };
            assert_eq!(reply_to.as_deref(), Some("earlier"));
            assert_eq!(reply_preview.as_deref(), Some("the earlier message"));
            let files = receive_batch(&connection, count, destination.to_str().unwrap(), false)
                .await
                .unwrap();
            assert_eq!(files.len(), 1);
            super::send_ack(&connection, &id, Ok(())).await.unwrap();
        };
        let paths = [source_file.to_string_lossy().into_owned()];
        let meta = v2_meta("m-4");
        let client = super::send_files_on_connection(
            &paths,
            &meta,
            super::ProgressSink::default(),
            super::connect_at(address, &[1; 16], &[2; 16]),
        );
        let ((), status) = tokio::join!(server, client);
        assert_eq!(status.unwrap(), super::SendStatus::Delivered);
        std::fs::remove_dir_all(source).unwrap();
        std::fs::remove_dir_all(destination).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn chat_event_round_trips_and_is_acknowledged() {
        use crate::lan::protocol::ChatEvent;

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            let Some(Message::Chat {
                kind,
                id,
                text,
                emoji,
            }) = connection.recv_message().await.unwrap()
            else {
                panic!("expected chat frame");
            };
            let event = ChatEvent::from_frame(&kind, id.clone(), text, emoji);
            super::send_ack(&connection, &id, Ok(())).await.unwrap();
            assert!(matches!(
                connection.recv_message().await.unwrap(),
                Some(Message::Done)
            ));
            event
        };
        let client = async {
            let connection = super::connect_at(address, &[1; 16], &[2; 16])
                .await
                .unwrap();
            super::send_chat_on_connection(
                &connection,
                ChatEvent::Edit {
                    id: "m-5".into(),
                    text: "corrected".into(),
                },
            )
            .await
        };
        let (event, status) = tokio::join!(server, client);
        assert_eq!(status.unwrap(), super::SendStatus::Delivered);
        assert_eq!(
            event,
            Some(ChatEvent::Edit {
                id: "m-5".into(),
                text: "corrected".into()
            })
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelling_mid_file_rejects_with_cancelled_and_receiver_cleans_up() {
        let source = test_directory("cancel-source");
        let destination = test_directory("cancel-destination");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let source_file = source.join("large.bin");
        // Sparse/lazily zeroed: large enough that it cannot finish before the
        // cancel lands, without writing it to disk first.
        std::fs::File::create(&source_file)
            .unwrap()
            .set_len(256 * 1024 * 1024)
            .unwrap();
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();

        let server = async {
            let (stream, _) = listener.accept().await.unwrap();
            let (connection, _) = Connection::from_incoming(stream, &[2; 16]).await.unwrap();
            let Some(Message::Batch { count, .. }) = connection.recv_message().await.unwrap()
            else {
                panic!("expected batch");
            };
            receive_batch(&connection, count, destination.to_str().unwrap(), false).await
        };
        let paths = [source_file.to_string_lossy().into_owned()];
        let meta = v2_meta("m-6");
        let client = super::with_cancel(
            Some(&cancel),
            super::send_files_on_connection(
                &paths,
                &meta,
                super::ProgressSink::default(),
                super::connect_at(address, &[1; 16], &[2; 16]),
            ),
        );
        // Cancel once the receiver has started writing the file.
        let canceller = async {
            loop {
                let receiving = std::fs::read_dir(&destination)
                    .unwrap()
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().ends_with(".part"));
                if receiving {
                    cancel.cancel();
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        };
        let (received, sent, ()) = tokio::join!(server, client, canceller);
        assert_eq!(sent.unwrap_err(), super::CANCELLED);
        assert!(received.is_err(), "receiver must see the incomplete file");
        assert_no_partial_files(&destination);
        assert!(!destination.join("large.bin").exists());
        std::fs::remove_dir_all(source).unwrap();
        std::fs::remove_dir_all(destination).unwrap();
    }

    #[test]
    fn control_frames_are_encoded_as_one_length_prefixed_buffer() {
        let frame = super::encode_frame(&Message::Done).unwrap();
        let json = br#"{"type":"done"}"#;
        assert_eq!(&frame[..4], &(json.len() as u32).to_be_bytes());
        assert_eq!(&frame[4..], json);
    }

    fn test_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("landrop-transfer-{label}-{}", uuid::Uuid::new_v4()))
    }

    /// Write one length-prefixed control frame from the test client side.
    async fn write_frame(client: &mut TcpStream, msg: &Message) {
        let json = serde_json::to_vec(msg).expect("encode test frame");
        let len = u32::try_from(json.len()).expect("frame length fits u32");
        client
            .write_all(&len.to_be_bytes())
            .await
            .expect("write frame length");
        client.write_all(&json).await.expect("write frame payload");
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
    fn deduplicates_existing_destination_names() {
        use super::deduplicate_path;

        let directory = test_directory("dedup");
        std::fs::create_dir_all(&directory).expect("create test directory");

        // Free name is returned unchanged.
        let free = directory.join("report.pdf");
        assert_eq!(deduplicate_path(&free).expect("free name"), free);

        // First collision becomes "report (1).pdf", then "report (2).pdf".
        std::fs::write(&free, b"a").expect("create collision");
        assert_eq!(
            deduplicate_path(&free)
                .expect("first dedup")
                .file_name()
                .and_then(|n| n.to_str()),
            Some("report (1).pdf")
        );
        std::fs::write(directory.join("report (1).pdf"), b"b").expect("create collision 2");
        assert_eq!(
            deduplicate_path(&free)
                .expect("second dedup")
                .file_name()
                .and_then(|n| n.to_str()),
            Some("report (2).pdf")
        );

        // Extensionless names keep the bare " (n)" suffix.
        let bare = directory.join("LICENSE");
        std::fs::write(&bare, b"c").expect("create bare collision");
        assert_eq!(
            deduplicate_path(&bare)
                .expect("bare dedup")
                .file_name()
                .and_then(|n| n.to_str()),
            Some("LICENSE (1)")
        );

        // Multi-dot names only replace the final extension segment.
        let tarball = directory.join("archive.tar.gz");
        std::fs::write(&tarball, b"d").expect("create tarball collision");
        assert_eq!(
            deduplicate_path(&tarball)
                .expect("tarball dedup")
                .file_name()
                .and_then(|n| n.to_str()),
            Some("archive.tar (1).gz")
        );

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn resolves_receive_base_dir_with_and_without_date_folder() {
        use super::resolve_receive_base_dir;

        let root = test_directory("base-dir");
        let folder = root.to_string_lossy().into_owned();

        assert_eq!(
            resolve_receive_base_dir(&folder, false, None).unwrap(),
            root
        );

        let dated = resolve_receive_base_dir(&folder, true, None).unwrap();
        assert_eq!(dated.parent(), Some(root.as_path()));
        let date_folder = dated
            .file_name()
            .and_then(|n| n.to_str())
            .expect("date folder name");
        // dd.mm.yyyy
        assert_eq!(date_folder.len(), 10);
        let parts: Vec<&str> = date_folder.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts[0].len() == 2 && parts[0].chars().all(|c| c.is_ascii_digit()));
        assert!(parts[1].len() == 2 && parts[1].chars().all(|c| c.is_ascii_digit()));
        assert!(parts[2].len() == 4 && parts[2].chars().all(|c| c.is_ascii_digit()));

        // An empty folder falls back to the platform Downloads directory.
        #[cfg(not(target_os = "android"))]
        assert!(!resolve_receive_base_dir("", false, None)
            .unwrap()
            .as_os_str()
            .is_empty());
        let private = root.join("files").join("received");
        assert_eq!(
            resolve_receive_base_dir("", false, Some(&private)).unwrap(),
            private
        );
        assert_eq!(
            resolve_receive_base_dir(&folder, false, Some(&private)).unwrap(),
            root
        );
    }

    #[test]
    fn progress_throttle_emits_on_time_or_percent_steps() {
        use super::ProgressThrottle;

        let mut throttle = ProgressThrottle::new(1_000_000);
        // First call has no previous emit: always due.
        assert!(throttle.should_emit(0));
        // A tiny step right after is neither 100 ms old nor 1% of the total.
        assert!(!throttle.should_emit(1));
        // Crossing 1% (10_000 bytes) emits regardless of elapsed time.
        assert!(throttle.should_emit(10_001));

        // A zero-byte total must not divide by zero and must stay emit-capable.
        let mut empty = ProgressThrottle::new(0);
        assert!(empty.should_emit(0));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn receives_batch_with_directory_markers() {
        let directory = test_directory("batch-happy");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let folder = directory.to_string_lossy().into_owned();

        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let send = async {
            // Item 1 arrives inside a folder, item 2 is a loose file.
            write_frame(
                &mut client,
                &Message::Dir {
                    name: "docs".into(),
                },
            )
            .await;
            write_frame(
                &mut client,
                &Message::File {
                    name: "docs/a.txt".into(),
                    size: 5,
                },
            )
            .await;
            client.write_all(b"hello").await.expect("write a.txt body");
            write_frame(
                &mut client,
                &Message::File {
                    name: "b.txt".into(),
                    size: 3,
                },
            )
            .await;
            client.write_all(b"abc").await.expect("write b.txt body");
            write_frame(&mut client, &Message::Done).await;
            client.shutdown().await.expect("close client stream");
        };

        let (files, ()) = tokio::join!(receive_batch(&connection, 2, &folder, false), send);
        let files = files.expect("batch receive succeeds");

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "docs/a.txt");
        assert_eq!(files[0].2, 5);
        assert_eq!(files[1].0, "b.txt");
        assert_eq!(files[1].2, 3);
        assert_eq!(
            std::fs::read(directory.join("docs").join("a.txt")).expect("read nested file"),
            b"hello"
        );
        assert_eq!(
            std::fs::read(directory.join("b.txt")).expect("read loose file"),
            b"abc"
        );
        assert_no_partial_files(&directory);

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn batch_rejects_early_done() {
        let directory = test_directory("batch-early-done");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let folder = directory.to_string_lossy().into_owned();

        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let send = async {
            write_frame(
                &mut client,
                &Message::File {
                    name: "only.txt".into(),
                    size: 2,
                },
            )
            .await;
            client.write_all(b"ok").await.expect("write body");
            // Promised two files, terminates after one.
            write_frame(&mut client, &Message::Done).await;
            client.shutdown().await.expect("close client stream");
        };

        let (result, ()) = tokio::join!(receive_batch(&connection, 2, &folder, false), send);
        let error = result.expect_err("early Done must fail the batch");
        assert!(
            error.contains("Batch ended early after 1 of 2 files"),
            "{error}"
        );
        assert_no_partial_files(&directory);

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn batch_rejects_unexpected_message_kind() {
        let directory = test_directory("batch-wrong-kind");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let folder = directory.to_string_lossy().into_owned();

        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let send = async {
            write_frame(
                &mut client,
                &Message::Text {
                    text: "not a file".into(),
                    id: None,
                    reply_to: None,
                    reply_preview: None,
                    sent_at: None,
                },
            )
            .await;
            client.shutdown().await.expect("close client stream");
        };

        let (result, ()) = tokio::join!(receive_batch(&connection, 1, &folder, false), send);
        let error = result.expect_err("a text frame is not a batch item");
        assert!(
            error.contains("Unexpected text message in batch"),
            "{error}"
        );

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn batch_requires_a_done_terminator() {
        let directory = test_directory("batch-no-done");
        std::fs::create_dir_all(&directory).expect("create test directory");
        let folder = directory.to_string_lossy().into_owned();

        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let send = async {
            write_frame(
                &mut client,
                &Message::File {
                    name: "one.txt".into(),
                    size: 1,
                },
            )
            .await;
            client.write_all(b"x").await.expect("write body");
            // Wrong terminator.
            write_frame(
                &mut client,
                &Message::Text {
                    text: "bye".into(),
                    id: None,
                    reply_to: None,
                    reply_preview: None,
                    sent_at: None,
                },
            )
            .await;
            client.shutdown().await.expect("close client stream");
        };

        let (result, ()) = tokio::join!(receive_batch(&connection, 1, &folder, false), send);
        let error = result.expect_err("batch must end with Done");
        assert!(error.contains("Expected batch completion"), "{error}");

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn accepts_a_frame_of_exactly_the_maximum_length() {
        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let send = async {
            let len = u32::try_from(MAX_CONTROL_FRAME_BYTES).expect("max fits u32");
            client
                .write_all(&len.to_be_bytes())
                .await
                .expect("write frame length");
            client
                .write_all(&vec![b'x'; MAX_CONTROL_FRAME_BYTES])
                .await
                .expect("write frame payload");
            client.shutdown().await.expect("close client stream");
        };

        let (message, ()) = tokio::join!(connection.recv_message(), send);
        // The length is accepted (not "too large"); the payload then fails to parse.
        let error = message.expect_err("payload is not valid JSON");
        assert!(error.contains("Invalid control message"), "{error}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_a_frame_one_byte_over_the_maximum() {
        let (mut client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);

        let send = async {
            let len = u32::try_from(MAX_CONTROL_FRAME_BYTES + 1).expect("max + 1 fits u32");
            client
                .write_all(&len.to_be_bytes())
                .await
                .expect("write frame length");
            client.shutdown().await.expect("close client stream");
        };

        let (message, ()) = tokio::join!(connection.recv_message(), send);
        let error = message.expect_err("oversized frame must be rejected");
        assert!(error.contains("Control message too large"), "{error}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn clean_eof_before_any_frame_is_not_an_error() {
        // Guards the discovery-probe case: a peer that connects and closes
        // without sending a control frame is a probe, not a failed transfer.
        let (client, server) = tcp_pair().await;
        let connection = Connection::from_stream(server);
        drop(client);

        let message = connection.recv_message().await.expect("clean EOF is Ok");
        assert!(message.is_none());
    }

    #[test]
    fn rejects_excessive_control_values() {
        assert!(validate_message(&Message::Batch {
            count: MAX_BATCH_FILES + 1,
            id: None,
            reply_to: None,
            reply_preview: None,
        })
        .is_err());
        assert!(validate_message(&Message::Text {
            text: "x".repeat(MAX_TEXT_BYTES + 1),
            id: None,
            reply_to: None,
            reply_preview: None,
            sent_at: None,
        })
        .is_err());
        assert!(validate_message(&Message::Text {
            text: "ok".into(),
            id: Some("x".repeat(129)),
            reply_to: None,
            reply_preview: None,
            sent_at: None,
        })
        .is_err());
        assert!(validate_message(&Message::Chat {
            kind: "react".into(),
            id: "m1".into(),
            text: None,
            emoji: Some("x".repeat(65)),
        })
        .is_err());
    }
}
