//! File metadata, previews, raw reads, and thumbnails.

use base64::Engine;
use serde::Serialize;
use std::path::Path;

/// Decode limits for thumbnail generation (≈64 MP, 512 MB working memory) —
/// a crafted 20k×20k PNG would otherwise allocate ~1.6 GB before `thumbnail()`.
const THUMBNAIL_MAX_DIMENSION_PX: u32 = 8192;
const THUMBNAIL_MAX_ALLOC_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Serialize)]
pub struct FileInfo {
    pub name: String,
    /// Raw byte count — the frontend formats for display (fileSizeStr).
    pub size_bytes: u64,
    #[serde(rename = "type")]
    pub file_type: String,
    pub count: Option<usize>,
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
                size_bytes: total_size,
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
                size_bytes: meta.len(),
                file_type: format!(".{}", ext),
                count: None,
            })
        }
    })
    .await
    .map_err(|e| e.to_string())?
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

        // Bound decoding of untrusted received images before `thumbnail()`.
        let Ok(reader) = image::ImageReader::open(p) else {
            return Ok(None);
        };
        let Ok(mut reader) = reader.with_guessed_format() else {
            return Ok(None);
        };
        let mut limits = image::Limits::no_limits();
        limits.max_image_width = Some(THUMBNAIL_MAX_DIMENSION_PX);
        limits.max_image_height = Some(THUMBNAIL_MAX_DIMENSION_PX);
        limits.max_alloc = Some(THUMBNAIL_MAX_ALLOC_BYTES);
        reader.limits(limits);

        match reader.decode() {
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

#[derive(Serialize)]
pub struct FilePreview {
    pub name: String,
    /// Raw byte count — the frontend formats for display (fileSizeStr).
    pub size_bytes: u64,
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
        let size_bytes = meta.len();
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
                        size_bytes,
                        extension: ext,
                        content: Some(preview),
                        line_count: total,
                        truncated,
                    })
                }
                Err(_) => Ok(FilePreview {
                    name,
                    size_bytes,
                    extension: ext,
                    content: None,
                    line_count: 0,
                    truncated: false,
                }),
            }
        } else {
            Ok(FilePreview {
                name,
                size_bytes,
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
pub async fn read_file_bytes(path: String) -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        if !p.exists() || !p.is_file() {
            return Err("File not found".to_string());
        }

        std::fs::read(p).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
