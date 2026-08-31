//! Clipboard access: pasted images and copied file lists.

use base64::Engine;

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
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| e.to_string())?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let filename = format!("paste_{}.{}", ts, ext);
    let file_path = temp_dir.join(&filename);
    tokio::fs::write(&file_path, &bytes)
        .await
        .map_err(|e| e.to_string())?;

    Ok(file_path.to_string_lossy().to_string())
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
