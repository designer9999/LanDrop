use crate::lan::LanState;
use base64::Engine;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

#[derive(Serialize)]
pub struct ReceiveFolderSettingsResponse {
    pub default_out_folder: String,
    pub peer_folders: HashMap<String, String>,
    pub sort_by_date: bool,
}

#[tauri::command]
pub async fn get_status() -> Result<StatusResponse, String> {
    let local_ip = get_local_ip_inner();
    Ok(StatusResponse {
        ok: true,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        local_ip,
    })
}

#[tauri::command]
pub async fn start_lan_service(state: State<'_, LanState>) -> Result<(), String> {
    state.service.start().await
}

#[tauri::command]
pub async fn stop_lan_service(state: State<'_, LanState>) -> Result<(), String> {
    state.service.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn lan_send_text(
    peer_id: String,
    text: String,
    peer_ip: Option<String>,
    state: State<'_, LanState>,
) -> Result<bool, String> {
    state
        .service
        .send_text(&peer_id, peer_ip.as_deref(), &text)
        .await
}

#[tauri::command]
pub async fn lan_send_files(
    peer_id: String,
    paths: Vec<String>,
    peer_ip: Option<String>,
    state: State<'_, LanState>,
) -> Result<bool, String> {
    state
        .service
        .send_files(&peer_id, peer_ip.as_deref(), &paths)
        .await
}

#[tauri::command]
pub async fn set_default_out_folder(
    folder: String,
    state: State<'_, LanState>,
) -> Result<(), String> {
    state.service.set_default_folder(&folder).await;
    Ok(())
}

#[tauri::command]
pub async fn set_peer_out_folder(
    peer_id: String,
    folder: String,
    state: State<'_, LanState>,
) -> Result<(), String> {
    state.service.set_peer_folder(&peer_id, &folder).await;
    Ok(())
}

#[tauri::command]
pub async fn get_receive_folder_settings(
    state: State<'_, LanState>,
) -> Result<ReceiveFolderSettingsResponse, String> {
    let (default_out_folder, peer_folders, sort_by_date) =
        state.service.get_folder_settings().await;
    Ok(ReceiveFolderSettingsResponse {
        default_out_folder,
        peer_folders,
        sort_by_date,
    })
}

#[tauri::command]
pub async fn set_receive_sort_by_date(
    enabled: bool,
    state: State<'_, LanState>,
) -> Result<(), String> {
    state.service.set_sort_by_date(enabled).await;
    Ok(())
}

#[tauri::command]
pub async fn set_device_alias(alias: String, state: State<'_, LanState>) -> Result<(), String> {
    state.service.set_alias(&alias).await;
    Ok(())
}

#[tauri::command]
pub async fn get_device_identity(state: State<'_, LanState>) -> Result<serde_json::Value, String> {
    let id = state.service.get_identity().await;
    Ok(serde_json::json!({
        "id": id.id,
        "alias": id.alias,
        "device_type": id.device_type,
    }))
}

#[tauri::command]
pub async fn get_file_info(path: String) -> Result<FileInfo, String> {
    tokio::task::spawn_blocking(move || {
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
                name: p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("folder")
                    .to_string(),
                size: format_size(total_size),
                file_type: "folder".to_string(),
                count: Some(count),
            })
        } else {
            let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            Ok(FileInfo {
                name: p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string(),
                size: format_size(meta.len()),
                file_type: format!(".{}", ext),
                count: None,
            })
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<bool, String> {
    let requested = PathBuf::from(path);
    let target = if requested.exists() {
        requested
    } else if let Some(parent) = requested.parent().filter(|parent| parent.exists()) {
        parent.to_path_buf()
    } else {
        return Err("Path does not exist".to_string());
    };

    #[cfg(target_os = "windows")]
    {
        let canonical = target.canonicalize().unwrap_or(target.clone());
        let mut command = std::process::Command::new("explorer");

        if canonical.is_file() {
            // /select,<path> opens the parent folder and highlights the file.
            let select_arg = format!("/select,{}", canonical.to_string_lossy());
            command.arg(select_arg);
        } else {
            command.arg(canonical.to_string_lossy().to_string());
        }

        command.spawn().map_err(|e| e.to_string())?;
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

    // Run image processing in blocking thread
    let result = tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let p = Path::new(&path);
        if !p.exists() {
            return Ok(None);
        }

        let ext = p
            .extension()
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
pub async fn read_file_preview(
    path: String,
    max_lines: Option<usize>,
) -> Result<FilePreview, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        if !p.exists() || !p.is_file() {
            return Err("File not found".to_string());
        }

        let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let size = format_size(meta.len());
        let max = max_lines.unwrap_or(200);

        let text_exts = [
            "txt",
            "md",
            "html",
            "htm",
            "css",
            "js",
            "ts",
            "jsx",
            "tsx",
            "json",
            "xml",
            "yaml",
            "yml",
            "toml",
            "ini",
            "cfg",
            "conf",
            "log",
            "csv",
            "py",
            "rs",
            "go",
            "java",
            "c",
            "cpp",
            "h",
            "hpp",
            "cs",
            "rb",
            "php",
            "sh",
            "bash",
            "zsh",
            "bat",
            "ps1",
            "sql",
            "svelte",
            "vue",
            "env",
            "gitignore",
            "dockerfile",
            "makefile",
        ];

        let is_text = text_exts.contains(&ext.as_str()) || meta.len() < 64 * 1024;

        if is_text {
            match std::fs::read_to_string(p) {
                Ok(content) => {
                    let lines: Vec<&str> = content.lines().collect();
                    let total = lines.len();
                    let truncated = total > max;
                    let preview: String =
                        lines.into_iter().take(max).collect::<Vec<_>>().join("\n");
                    Ok(FilePreview {
                        name,
                        size,
                        extension: ext,
                        content: Some(preview),
                        line_count: total,
                        truncated,
                    })
                }
                Err(_) => Ok(FilePreview {
                    name,
                    size,
                    extension: ext,
                    content: None,
                    line_count: 0,
                    truncated: false,
                }),
            }
        } else {
            Ok(FilePreview {
                name,
                size,
                extension: ext,
                content: None,
                line_count: 0,
                truncated: false,
            })
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_clipboard_files() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        use clipboard_win::{formats, Clipboard};
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
    crate::lan::discovery::get_local_ipv4()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[tauri::command]
pub async fn set_mica(handle: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        let window = handle.get_webview_window("main").ok_or("No main window")?;
        if enabled {
            // Minimal tint — let CSS control the darkness
            let tint: window_vibrancy::Color = (0, 0, 0, 1);
            if window_vibrancy::apply_acrylic(&window, Some(tint)).is_err() {
                let _ = window_vibrancy::apply_mica(&window, Some(true));
            }
        } else {
            let _ = window_vibrancy::clear_acrylic(&window);
            let _ = window_vibrancy::clear_mica(&window);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (handle, enabled);
    }
    Ok(())
}

/// Get currently selected files from the active Windows Explorer window via native COM.
/// Uses IShellWindows → IWebBrowser2 → IShellBrowser → IFolderView2 → IShellItemArray.
#[tauri::command]
pub async fn get_explorer_selection() -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    {
        // COM shell objects require STA — run on a dedicated thread
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let result = explorer_selection_com();
            let _ = tx.send(result);
        });
        rx.await.map_err(|_| "Thread join failed".to_string())?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(vec![])
    }
}

#[cfg(target_os = "windows")]
fn explorer_selection_com() -> Result<Vec<String>, String> {
    use windows::core::Interface as _;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IServiceProvider, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        IFolderView2, IShellBrowser, IShellItem, IShellItemArray, IShellView, IShellWindows,
        IWebBrowser2, SID_STopLevelBrowser, ShellWindows, SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let result = (|| -> Result<Vec<String>, String> {
            let shell_windows: IShellWindows =
                CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).map_err(|e| format!("{e}"))?;

            let fg_hwnd = GetForegroundWindow();
            let count = shell_windows.Count().map_err(|e| format!("{e}"))?;

            for i in 0..count {
                // Build VARIANT with VT_I4 for the index
                let v = windows::Win32::System::Variant::VARIANT::from(i);

                let Ok(disp) = shell_windows.Item(&v) else {
                    continue;
                };
                let Ok(wb) = disp.cast::<IWebBrowser2>() else {
                    continue;
                };
                let Ok(hwnd_val) = wb.HWND() else { continue };

                let wnd = HWND(hwnd_val.0 as *mut _);
                if wnd != fg_hwnd {
                    continue;
                }

                let sp: IServiceProvider = wb.cast().map_err(|e| format!("{e}"))?;
                let sb: IShellBrowser = sp
                    .QueryService(&SID_STopLevelBrowser)
                    .map_err(|e| format!("{e}"))?;
                let sv: IShellView = sb.QueryActiveShellView().map_err(|e| format!("{e}"))?;
                let fv: IFolderView2 = sv.cast().map_err(|e| format!("{e}"))?;
                let selection: IShellItemArray =
                    fv.GetSelection(false).map_err(|e| format!("{e}"))?;
                let n = selection.GetCount().map_err(|e| format!("{e}"))?;

                let mut paths = Vec::with_capacity(n as usize);
                for j in 0..n {
                    let item: IShellItem = match selection.GetItemAt(j) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let name_ptr: windows::core::PWSTR =
                        match item.GetDisplayName(SIGDN_FILESYSPATH) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                    let result = name_ptr.to_string();
                    // Free COM-allocated PWSTR to prevent memory leak
                    windows::Win32::System::Com::CoTaskMemFree(Some(name_ptr.0 as *const _));
                    if let Ok(s) = result {
                        if !s.is_empty() {
                            paths.push(s);
                        }
                    }
                }
                return Ok(paths);
            }
            Ok(vec![])
        })();

        CoUninitialize();
        result
    }
}

/// Open a file with the system's default handler
#[tauri::command]
pub async fn open_file(path: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let target = PathBuf::from(&path);

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if !target.exists() {
            return Err("open_file: path does not exist".to_string());
        }
        if !target.is_dir() {
            return Err("open_file: desktop open is restricted to folders".to_string());
        }
    }

    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| format!("open_file: {e}"))
}

/// Save raw bytes (from frontend file read) to a temp file for sending.
/// Used on Android where content:// URIs can't be read directly by Rust.
#[tauri::command]
pub async fn save_temp_for_send(
    name: String,
    data: Vec<u8>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use tauri::Manager;
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let send_dir = cache_dir.join("send_cache");
    tokio::fs::create_dir_all(&send_dir)
        .await
        .map_err(|e| e.to_string())?;
    let out_path = send_dir.join(&name);
    tokio::fs::write(&out_path, &data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(out_path.to_string_lossy().to_string())
}

/// Clean up temp files from send cache
#[tauri::command]
pub async fn cleanup_send_cache(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let send_dir = cache_dir.join("send_cache");
    if send_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&send_dir).await;
    }
    Ok(())
}

pub(crate) fn format_size(bytes: u64) -> String {
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
