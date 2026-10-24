//! Clipboard access: pasted images and copied file lists.

use tauri::ipc::{InvokeBody, Request};

/// Largest raw body accepted by the byte-upload commands.
const MAX_UPLOAD_BYTES: usize = 512 * 1024 * 1024;

/// Bytes of a raw-body command. Desktop delivers `InvokeBody::Raw`; Android's
/// IPC has no raw bodies, so the same JS call arrives as a JSON number array.
pub(crate) fn request_bytes(request: &Request<'_>) -> Result<Vec<u8>, String> {
    let bytes = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        InvokeBody::Json(serde_json::Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or_else(|| "Request body is not a byte array".to_string())
            })
            .collect::<Result<Vec<u8>, String>>()?,
        InvokeBody::Json(_) => return Err("Expected a raw byte request body".into()),
    };
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err("Request body is too large".into());
    }
    Ok(bytes)
}

/// A request header as text (`None` when absent or not visible ASCII).
pub(crate) fn request_header(request: &Request<'_>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn clipboard_image_extension(mime: &str) -> &'static str {
    match mime.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "png",
    }
}

/// Write pasted image bytes to a new, unpredictably named file in the app's
/// own cache directory (never shared /tmp, never a timestamp-only name).
async fn write_pasted_image(
    cache_dir: &std::path::Path,
    bytes: &[u8],
    mime: &str,
) -> Result<std::path::PathBuf, String> {
    let directory = cache_dir.join("paste");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let path = directory.join(format!(
        "paste_{}.{}",
        uuid::Uuid::new_v4().simple(),
        clipboard_image_extension(mime)
    ));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_err(|e| e.to_string())?;
    let written = async {
        use tokio::io::AsyncWriteExt;
        file.write_all(bytes).await?;
        file.flush().await
    }
    .await;
    if let Err(error) = written {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        return Err(error.to_string());
    }
    Ok(path)
}

/// Raw request body = image bytes; header `x-mime` = image MIME type.
#[tauri::command]
pub async fn save_clipboard_image(
    request: Request<'_>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use tauri::Manager;
    let bytes = request_bytes(&request)?;
    if bytes.is_empty() {
        return Err("Pasted image is empty".into());
    }
    let mime = request_header(&request, "x-mime").unwrap_or_default();
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let path = write_pasted_image(&cache_dir, &bytes, &mime).await?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_clipboard_files() -> Result<Vec<String>, String> {
    // Clipboard access is synchronous on every platform (and shells out to
    // wl-paste/xclip/osascript on Linux/macOS) — run it off the async workers.
    tokio::task::spawn_blocking(get_clipboard_files_blocking)
        .await
        .map_err(|e| e.to_string())?
}

fn get_clipboard_files_blocking() -> Result<Vec<String>, String> {
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
    #[cfg(target_os = "linux")]
    {
        Ok(linux_clipboard_files())
    }
    #[cfg(target_os = "macos")]
    {
        Ok(macos_clipboard_files())
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Ok(vec![])
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_clipboard_files() -> Vec<String> {
    // Try Wayland first via wl-paste, fall back to xclip on X11.
    // Both file managers (Nautilus, Dolphin etc.) put copied files in the
    // x-special/gnome-copied-files mime type as a list of file:// URIs.
    let try_clip = |cmd: &str, args: &[&str]| -> Option<String> {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok()
                } else {
                    None
                }
            })
    };

    let raw = try_clip("wl-paste", &["--type", "x-special/gnome-copied-files"])
        .or_else(|| {
            try_clip(
                "xclip",
                &[
                    "-selection",
                    "clipboard",
                    "-t",
                    "x-special/gnome-copied-files",
                    "-o",
                ],
            )
        })
        // KDE Plasma uses its own MIME type
        .or_else(|| try_clip("wl-paste", &["--type", "application/x-kde-cutselection"]))
        .or_else(|| {
            try_clip(
                "xclip",
                &[
                    "-selection",
                    "clipboard",
                    "-t",
                    "application/x-kde-cutselection",
                    "-o",
                ],
            )
        })
        // Generic uri-list (works for most file managers)
        .or_else(|| try_clip("wl-paste", &["--type", "text/uri-list"]))
        .or_else(|| {
            try_clip(
                "xclip",
                &["-selection", "clipboard", "-t", "text/uri-list", "-o"],
            )
        });

    let Some(content) = raw else {
        return vec![];
    };

    // First line may be "copy" or "cut" — skip it. Keep file:// URIs only.
    content
        .lines()
        .filter(|l| l.starts_with("file://"))
        .filter_map(|l| {
            let path = l.trim_start_matches("file://");
            // URL-decode percent-encoded chars
            let decoded = urlencoding::decode(path).ok()?;
            Some(decoded.to_string())
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn macos_clipboard_files() -> Vec<String> {
    // Read NSPasteboard via osascript — files copied in Finder appear as POSIX paths
    let output = std::process::Command::new("osascript")
        .args(["-e", "the clipboard as «class furl»"])
        .output();
    let Ok(out) = output else {
        return vec![];
    };
    if !out.status.success() {
        return vec![];
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // Output is "{«class furl»\"file:///path1\", «class furl»\"file:///path2\"}"
    text.split("file://")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .map(|s| {
            urlencoding::decode(s)
                .map(|c| c.to_string())
                .unwrap_or_else(|_| s.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn pasted_images_get_unique_unpredictable_names_in_the_cache() {
        let cache = std::env::temp_dir().join(format!("landrop-paste-{}", uuid::Uuid::new_v4()));
        let first = super::write_pasted_image(&cache, b"one", "image/jpeg")
            .await
            .unwrap();
        let second = super::write_pasted_image(&cache, b"two", "image/jpeg")
            .await
            .unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with(cache.join("paste")));
        assert_eq!(first.extension().and_then(|e| e.to_str()), Some("jpg"));
        let stem = first.file_stem().and_then(|s| s.to_str()).unwrap();
        assert_eq!(stem.len(), "paste_".len() + 32, "{stem}");
        assert_eq!(tokio::fs::read(&first).await.unwrap(), b"one");
        assert_eq!(tokio::fs::read(&second).await.unwrap(), b"two");
        assert_eq!(super::clipboard_image_extension("text/html"), "png");
        tokio::fs::remove_dir_all(cache).await.unwrap();
    }
}
