use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

use super::protocol::{Message, CHUNK_SIZE};

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

    pub async fn from_incoming(mut stream: TcpStream, expected_hash: &[u8]) -> Result<Arc<Self>, String> {
        stream.set_nodelay(true).map_err(|e| e.to_string())?;

        let mut peer_hash = vec![0u8; 16];
        stream.read_exact(&mut peer_hash).await.map_err(|e| e.to_string())?;
        stream.write_all(expected_hash).await.map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;

        if peer_hash != expected_hash {
            return Err("Code mismatch".into());
        }

        Ok(Arc::new(Self::from_stream(stream)))
    }

    pub async fn from_outgoing(mut stream: TcpStream, hash: &[u8]) -> Result<Arc<Self>, String> {
        stream.set_nodelay(true).map_err(|e| e.to_string())?;

        stream.write_all(hash).await.map_err(|e| e.to_string())?;
        stream.flush().await.map_err(|e| e.to_string())?;
        let mut peer_hash = vec![0u8; 16];
        stream.read_exact(&mut peer_hash).await.map_err(|e| e.to_string())?;

        if peer_hash != hash {
            return Err("Code mismatch".into());
        }

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
        r.read_exact(&mut len_buf).await.map_err(|e| e.to_string())?;
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

    /// Acquire the writer lock for the caller. Used by send_files to hold
    /// the lock across message + raw data to prevent protocol interleaving.
    pub async fn lock_writer(&self) -> tokio::sync::MutexGuard<'_, OwnedWriteHalf> {
        self.writer.lock().await
    }
}

/// Write a framed message directly to a writer (caller already holds the lock)
async fn write_message(w: &mut OwnedWriteHalf, msg: &Message) -> Result<(), String> {
    let json = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
    let len = (json.len() as u32).to_be_bytes();
    w.write_all(&len).await.map_err(|e| e.to_string())?;
    w.write_all(&json).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn send_files(conn: &Connection, paths: &[String], handle: Option<&AppHandle>) -> Result<(), String> {
    // Collect all files to send (skip symlinks for security)
    let mut file_entries: Vec<(String, PathBuf)> = Vec::new();

    for path_str in paths {
        let path = Path::new(path_str);

        if path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            continue;
        }

        if path.is_dir() {
            let dir_name = path.file_name()
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
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            file_entries.push((name, path.to_path_buf()));
        }
    }

    if file_entries.is_empty() {
        return Ok(());
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

    if let Some(h) = handle {
        let _ = h.emit("lan_transfer_progress", serde_json::json!({
            "direction": "send",
            "phase": "start",
            "total_bytes": total_size,
            "total_files": total_files,
            "sent_bytes": 0,
            "sent_files": 0,
        }));
    }

    // *** HOLD THE WRITER LOCK FOR THE ENTIRE TRANSFER ***
    // This prevents ping/pong from interleaving with file data
    let mut w = conn.lock_writer().await;

    // Send batch header if multiple files
    if file_entries.len() > 1 {
        write_message(&mut w, &Message::Batch { count: file_entries.len() as u32 }).await?;
    }

    let mut sent_bytes: u64 = 0;
    let mut sent_files: usize = 0;

    for (name, file_path) in &file_entries {
        let metadata = tokio::fs::metadata(file_path).await.map_err(|e| e.to_string())?;
        let size = metadata.len();

        // Send dir marker
        if name.contains('/') {
            let dir_part = &name[..name.rfind('/').unwrap()];
            write_message(&mut w, &Message::Dir { name: dir_part.to_string() }).await?;
        }

        // Send file header + raw data atomically (no interleaving possible)
        write_message(&mut w, &Message::File { name: name.clone(), size }).await?;

        let mut file = tokio::fs::File::open(file_path).await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut remaining = size;

        while remaining > 0 {
            let to_read = std::cmp::min(remaining as usize, CHUNK_SIZE);
            let n = file.read(&mut buf[..to_read]).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
            remaining -= n as u64;
            sent_bytes += n as u64;

            if let Some(h) = handle {
                let _ = h.emit("lan_transfer_progress", serde_json::json!({
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

        // Flush after each file
        w.flush().await.map_err(|e| e.to_string())?;
        sent_files += 1;
    }

    // Send done marker for batch
    if file_entries.len() > 1 {
        write_message(&mut w, &Message::Done).await?;
        w.flush().await.map_err(|e| e.to_string())?;
    }

    // Drop the writer lock — pings can resume
    drop(w);

    if let Some(h) = handle {
        let _ = h.emit("lan_transfer_progress", serde_json::json!({
            "direction": "send",
            "phase": "done",
            "total_bytes": total_size,
            "total_files": total_files,
            "sent_bytes": sent_bytes,
            "sent_files": sent_files,
        }));
    }

    Ok(())
}

pub async fn receive_file(conn: &Connection, name: &str, size: u64, out_folder: &str, handle: Option<&AppHandle>) -> Result<String, String> {
    let safe_name = sanitize_path(name);
    let base_dir = if out_folder.is_empty() {
        PathBuf::from(dirs_next_downloads())
    } else {
        PathBuf::from(out_folder)
    };

    let out_path = base_dir.join(&safe_name);

    let canonical_base = base_dir.canonicalize().unwrap_or_else(|_| base_dir.clone());
    if let Ok(canonical_out) = out_path.canonicalize() {
        if !canonical_out.starts_with(&canonical_base) {
            return Err("Path traversal detected".into());
        }
    }
    if let Some(parent) = out_path.parent() {
        if parent.exists() {
            let canonical_parent = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
            if !canonical_parent.starts_with(&canonical_base) {
                return Err("Path traversal detected".into());
            }
        }
    }

    check_disk_space(&out_path, size)?;

    if let Some(parent) = out_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }

    let mut file = tokio::fs::File::create(&out_path).await.map_err(|e| e.to_string())?;
    let mut remaining = size;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut received_bytes: u64 = 0;

    while remaining > 0 {
        let to_read = std::cmp::min(remaining as usize, CHUNK_SIZE);
        conn.recv_raw(&mut buf[..to_read]).await?;
        file.write_all(&buf[..to_read]).await.map_err(|e| e.to_string())?;
        remaining -= to_read as u64;
        received_bytes += to_read as u64;

        if let Some(h) = handle {
            let _ = h.emit("lan_transfer_progress", serde_json::json!({
                "direction": "receive",
                "phase": "transferring",
                "total_bytes": size,
                "received_bytes": received_bytes,
                "current_file": name,
            }));
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    Ok(out_path.to_string_lossy().to_string())
}

pub async fn receive_batch(conn: &Connection, count: u32, out_folder: &str, handle: Option<&AppHandle>) -> Result<Vec<(String, String, u64)>, String> {
    let mut files = Vec::new();

    if let Some(h) = handle {
        let _ = h.emit("lan_transfer_progress", serde_json::json!({
            "direction": "receive",
            "phase": "start",
            "total_files": count,
            "received_files": 0,
        }));
    }

    for _ in 0..count {
        let msg = conn.recv_message().await?;
        match msg {
            Message::Dir { .. } => {
                let file_msg = conn.recv_message().await?;
                if let Message::File { name, size } = file_msg {
                    let path = receive_file(conn, &name, size, out_folder, handle).await?;
                    files.push((name, path, size));
                }
            }
            Message::File { name, size } => {
                let path = receive_file(conn, &name, size, out_folder, handle).await?;
                files.push((name, path, size));
            }
            Message::Done => break,
            _ => {}
        }

        if let Some(h) = handle {
            let _ = h.emit("lan_transfer_progress", serde_json::json!({
                "direction": "receive",
                "phase": "transferring",
                "total_files": count,
                "received_files": files.len(),
            }));
        }
    }

    if files.len() == count as usize {
        let _ = conn.recv_message().await;
    }

    if let Some(h) = handle {
        let _ = h.emit("lan_transfer_progress", serde_json::json!({
            "direction": "receive",
            "phase": "done",
            "total_files": count,
            "received_files": files.len(),
        }));
    }

    Ok(files)
}

fn sanitize_path(name: &str) -> String {
    let cleaned = name
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_string();

    let parts: Vec<&str> = cleaned
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".." && *p != ".")
        .collect();

    parts
        .iter()
        .map(|p| {
            p.chars()
                .map(|c| {
                    if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ' | '(' | ')' | '[' | ']') {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn check_disk_space(path: &Path, needed: u64) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let dir = path.parent().unwrap_or(path);
        let dir_str = dir.to_string_lossy().to_string();
        let wide: Vec<u16> = OsStr::new(&dir_str).encode_wide().chain(std::iter::once(0)).collect();

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

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (path, needed);
    }

    Ok(())
}

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

fn dirs_next_downloads() -> String {
    if let Some(user_dirs) = directories::UserDirs::new() {
        if let Some(downloads) = user_dirs.download_dir() {
            return downloads.to_string_lossy().to_string();
        }
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}
