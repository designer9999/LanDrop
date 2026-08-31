//! App-owned storage: Android send cache and durable media history copies.

use crate::path_utils::sanitize_file_name;
use std::path::PathBuf;

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
    let out_path = send_dir.join(sanitize_file_name(&name));
    tokio::fs::write(&out_path, &data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(out_path.to_string_lossy().to_string())
}

/// Save a durable local copy for message history previews.
/// Android picker URIs and cache files cannot be trusted after process restart.
#[tauri::command]
pub async fn save_history_file(
    name: String,
    data: Vec<u8>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use tauri::Manager;
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let history_dir = data_dir.join("media_history");
    tokio::fs::create_dir_all(&history_dir)
        .await
        .map_err(|e| e.to_string())?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_path = history_dir.join(format!("{}_{}", ts, sanitize_file_name(&name)));
    tokio::fs::write(&out_path, &data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(out_path.to_string_lossy().to_string())
}

/// Delete only files created in LanDrop's own media_history folder.
#[tauri::command]
pub async fn delete_history_files(paths: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let history_dir = data_dir.join("media_history");
    let history_root = tokio::fs::canonicalize(&history_dir)
        .await
        .unwrap_or(history_dir);

    for path in paths {
        let candidate = PathBuf::from(path);
        let Ok(target) = tokio::fs::canonicalize(candidate).await else {
            continue;
        };
        if target.starts_with(&history_root) && target.is_file() {
            let _ = tokio::fs::remove_file(target).await;
        }
    }

    Ok(())
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
