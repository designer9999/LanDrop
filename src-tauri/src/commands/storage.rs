//! App-owned storage: Android send cache and durable media history copies.

use crate::path_utils::sanitize_file_name;
use std::path::PathBuf;

fn history_directory(data_dir: PathBuf) -> PathBuf {
    #[cfg(target_os = "android")]
    let data_dir = data_dir.join("files");
    data_dir.join("media_history")
}

fn history_file_name(name: &str) -> String {
    format!("{}_{}", uuid::Uuid::new_v4(), sanitize_file_name(name))
}

async fn stage_send_file(
    cache_dir: &std::path::Path,
    name: &str,
    data: &[u8],
) -> Result<PathBuf, String> {
    // Preserve the display name on the wire without collisions between picks.
    let directory = cache_dir
        .join("send_cache")
        .join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let path = directory.join(sanitize_file_name(name));
    if let Err(error) = tokio::fs::write(&path, data).await {
        let _ = tokio::fs::remove_dir_all(&directory).await;
        return Err(error.to_string());
    }
    Ok(path)
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
    let out_path = stage_send_file(&cache_dir, &name, &data).await?;
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
    let history_dir = history_directory(data_dir);
    tokio::fs::create_dir_all(&history_dir)
        .await
        .map_err(|e| e.to_string())?;

    let out_path = history_dir.join(history_file_name(&name));
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
    let history_dir = history_directory(data_dir);
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

#[cfg(test)]
mod tests {
    #[test]
    fn history_names_are_unique_and_cannot_escape_the_history_directory() {
        let first = super::history_file_name("../../report.txt");
        let second = super::history_file_name("../../report.txt");
        assert_ne!(first, second);
        assert!(first.ends_with("_report.txt"));
        assert_eq!(std::path::Path::new(&first).components().count(), 1);
    }

    #[tokio::test]
    async fn same_name_documents_keep_distinct_bytes_and_safe_names() {
        let directory =
            std::env::temp_dir().join(format!("landrop-stage-test-{}", uuid::Uuid::new_v4()));
        let first = super::stage_send_file(&directory, "report.txt", b"first")
            .await
            .unwrap();
        let second = super::stage_send_file(&directory, "report.txt", b"second")
            .await
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(first.file_name(), second.file_name());
        assert_eq!(tokio::fs::read(&first).await.unwrap(), b"first");
        assert_eq!(tokio::fs::read(&second).await.unwrap(), b"second");
        let hostile = super::stage_send_file(&directory, "../../outside.txt", b"safe")
            .await
            .unwrap();
        assert!(hostile.starts_with(directory.join("send_cache")));
        assert_eq!(
            hostile.parent().unwrap().parent().unwrap(),
            directory.join("send_cache")
        );
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }
}
