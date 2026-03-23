use crate::lan::LanState;
use base64::Engine;
use serde::Serialize;
use std::path::Path;
use tauri::State;

#[derive(Serialize)]
pub struct StatusResponse {
    pub ok: bool,
    pub app_version: String,
    pub local_ip: String,
}

#[derive(Serialize)]
pub struct FileInfo {
    pub name: String,
    pub size: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub count: Option<usize>,
}

#[tauri::command]
pub async fn get_status() -> Result<StatusResponse, String> {
    let local_ip = get_local_ip_inner();
    Ok(StatusResponse {
        ok: true,
        app_version: "1.0.0".to_string(),
        local_ip,
    })
}

#[tauri::command]
pub async fn start_lan(
    code: String,
    out_folder: String,
    state: State<'_, LanState>,
) -> Result<(), String> {
    let peer = state.peer.lock().await;
    peer.start(&code, &out_folder).await
}

#[tauri::command]
pub async fn stop_lan(state: State<'_, LanState>) -> Result<(), String> {
    let peer = state.peer.lock().await;
    peer.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn lan_send_text(text: String, state: State<'_, LanState>) -> Result<bool, String> {
    let peer = state.peer.lock().await;
    peer.send_text(&text).await
}

#[tauri::command]
pub async fn lan_send_files(paths: Vec<String>, state: State<'_, LanState>) -> Result<bool, String> {
    let peer = state.peer.lock().await;
    peer.send_files(&paths).await
}

#[tauri::command]
pub async fn get_file_info(path: String) -> Result<FileInfo, String> {
    let p = Path::new(&path);

    if p.is_dir() {
        let mut count = 0usize;
        let mut total_size = 0u64;
        for entry in walkdir::WalkDir::new(p).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                count += 1;
                total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(FileInfo {
            name: p.file_name().and_then(|n| n.to_str()).unwrap_or("folder").to_string(),
            size: format_size(total_size),
            file_type: "folder".to_string(),
            count: Some(count),
        })
    } else {
        let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        Ok(FileInfo {
            name: p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string(),
            size: format_size(meta.len()),
            file_type: format!(".{}", ext),
            count: None,
        })
    }
}

#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<bool, String> {
    let p = Path::new(&path);
    let target = if p.is_file() {
        p.parent().unwrap_or(p)
    } else {
        p
    };

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(target.to_string_lossy().to_string())
            .spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(target.to_string_lossy().to_string())
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(target.to_string_lossy().to_string())
            .spawn();
    }

    Ok(true)
}

#[tauri::command]
pub async fn get_thumbnail(path: String, max_px: Option<u32>) -> Result<Option<String>, String> {
    let max = max_px.unwrap_or(120);
    let path = path.clone();

    // Run image processing in blocking thread
    let result = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let p = Path::new(&path);
        if !p.exists() {
            return Ok(None);
        }

        let ext = p.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let image_exts = ["jpg", "jpeg", "png", "gif", "webp", "bmp", "ico"];
        if !image_exts.contains(&ext.as_str()) {
            return Ok(None);
        }

        match image::open(p) {
            Ok(img) => {
                let thumb = img.thumbnail(max, max);
                let mut buf = std::io::Cursor::new(Vec::new());
                thumb
                    .write_to(&mut buf, image::ImageFormat::Png)
                    .map_err(|e| e.to_string())?;
                let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
                Ok(Some(format!("data:image/png;base64,{}", b64)))
            }
            Err(_) => Ok(None),
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    result
}

#[tauri::command]
pub async fn save_clipboard_image(base64_data: String, mime: String) -> Result<String, String> {
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "png",
    };

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let temp_dir = std::env::temp_dir().join("landrop_paste");
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let filename = format!("paste_{}.{}", ts, ext);
    let file_path = temp_dir.join(&filename);
    std::fs::write(&file_path, &bytes).map_err(|e| e.to_string())?;

    Ok(file_path.to_string_lossy().to_string())
}

#[derive(Serialize)]
pub struct FilePreview {
    pub name: String,
    pub size: String,
    pub extension: String,
    pub content: Option<String>,
    pub line_count: usize,
    pub truncated: bool,
}

#[tauri::command]
pub async fn read_file_preview(path: String, max_lines: Option<usize>) -> Result<FilePreview, String> {
    let p = Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err("File not found".to_string());
    }

    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let size = format_size(meta.len());
    let max = max_lines.unwrap_or(200);

    // Try to read as text
    let text_exts = [
        "txt", "md", "html", "htm", "css", "js", "ts", "jsx", "tsx", "json",
        "xml", "yaml", "yml", "toml", "ini", "cfg", "conf", "log", "csv",
        "py", "rs", "go", "java", "c", "cpp", "h", "hpp", "cs", "rb",
        "php", "sh", "bash", "zsh", "bat", "ps1", "sql", "svelte", "vue",
        "env", "gitignore", "dockerfile", "makefile",
    ];

    let is_text = text_exts.contains(&ext.as_str()) || meta.len() < 64 * 1024;

    if is_text {
        match std::fs::read_to_string(p) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let total = lines.len();
                let truncated = total > max;
                let preview: String = lines.into_iter().take(max).collect::<Vec<_>>().join("\n");
                Ok(FilePreview {
                    name, size, extension: ext, content: Some(preview), line_count: total, truncated,
                })
            }
            Err(_) => Ok(FilePreview {
                name, size, extension: ext, content: None, line_count: 0, truncated: false,
            }),
        }
    } else {
        Ok(FilePreview {
            name, size, extension: ext, content: None, line_count: 0, truncated: false,
        })
    }
}

#[tauri::command]
pub async fn get_clipboard_files() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use clipboard_win::{Clipboard, formats};
        let _clip = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
        let file_list = formats::FileList;
        let mut output = Vec::new();
        match clipboard_win::get::<Vec<String>, _>(file_list) {
            Ok(files) => {
                for file in files {
                    output.push(file);
                }
                Ok(output)
            }
            Err(_) => Ok(vec![]),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn get_local_ip() -> Result<String, String> {
    Ok(get_local_ip_inner())
}

fn get_local_ip_inner() -> String {
    // Prefer physical LAN IPv4, skip VPN/virtual adapters
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();

    // First pass: find a common LAN IPv4 (192.168.x.x, 10.x.x.x, 172.16-31.x.x)
    // Skip typical VPN adapter names
    let vpn_keywords = ["tun", "tap", "wg", "vpn", "proton", "nord", "mullvad", "wireguard"];

    for (name, ip) in &interfaces {
        if !ip.is_ipv4() { continue; }
        let ip4 = match ip { std::net::IpAddr::V4(v4) => v4, _ => continue };
        if ip4.is_loopback() { continue; }

        let name_lower = name.to_lowercase();
        if vpn_keywords.iter().any(|k| name_lower.contains(k)) { continue; }

        let octets = ip4.octets();
        let is_lan = octets[0] == 192 && octets[1] == 168
            || octets[0] == 10
            || (octets[0] == 172 && (16..=31).contains(&octets[1]));

        if is_lan {
            return ip.to_string();
        }
    }

    // Fallback: any IPv4 that's not loopback
    for (_, ip) in &interfaces {
        if let std::net::IpAddr::V4(v4) = ip {
            if !v4.is_loopback() {
                return ip.to_string();
            }
        }
    }

    "unknown".to_string()
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
