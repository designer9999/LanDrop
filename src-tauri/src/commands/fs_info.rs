//! File metadata, previews, raw reads, and thumbnails.

use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use tauri::ipc::Response;
use tauri::{AppHandle, Manager};

/// Upper bound on bytes read for a text preview.
const PREVIEW_MAX_BYTES: usize = 256 * 1024;

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

const THUMBNAIL_DEFAULT_PX: u32 = 320;
const THUMBNAIL_MIN_PX: u32 = 64;
const THUMBNAIL_MAX_PX: u32 = 1024;
const THUMBNAIL_JPEG_QUALITY: u8 = 82;

/// Formats the bundled `image` features can decode for thumbnails.
const THUMBNAIL_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "webp", "bmp", "ico",
];
/// Formats the webview may display or play from the asset protocol.
const MEDIA_IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "apng", "gif", "webp", "bmp", "ico", "avif",
];
const MEDIA_VIDEO_EXTS: &[&str] = &["mp4", "m4v", "webm", "mov", "ogv", "mkv"];

fn lowercase_extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// The canonical path to add to the asset scope, if `path` is an existing
/// regular file (not a symlink, not a directory) with an image/video extension.
fn media_scope_path(path: &Path) -> Option<PathBuf> {
    let ext = lowercase_extension(path);
    if !MEDIA_IMAGE_EXTS.contains(&ext.as_str()) && !MEDIA_VIDEO_EXTS.contains(&ext.as_str()) {
        return None;
    }
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    std::fs::canonicalize(path).ok()
}

/// Make exact media files readable through the asset protocol
/// (`convertFileSrc`). The static scope is empty; only files passed here, and
/// generated thumbnails, become reachable. Returns the input paths allowed.
#[tauri::command]
pub async fn allow_media_paths(paths: Vec<String>, app: AppHandle) -> Result<Vec<String>, String> {
    let checked = tokio::task::spawn_blocking(move || {
        paths
            .into_iter()
            .filter_map(|path| media_scope_path(Path::new(&path)).map(|scoped| (path, scoped)))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    let scope = app.asset_protocol_scope();
    Ok(checked
        .into_iter()
        .filter(|(_, scoped)| scope.allow_file(scoped).is_ok())
        .map(|(path, _)| path)
        .collect())
}

/// Path of a cached JPEG thumbnail for `source`, generating it on a miss.
/// The cache key covers canonical path, size, modification time and `max_px`,
/// so an edited or replaced file gets a fresh thumbnail. Animated images use
/// their first frame. `None` for non-images and undecodable files.
fn cached_thumbnail(cache_dir: &Path, source: &Path, max_px: u32) -> Option<PathBuf> {
    if !THUMBNAIL_EXTS.contains(&lowercase_extension(source).as_str()) {
        return None;
    }
    let canonical = std::fs::canonicalize(source).ok()?;
    let metadata = std::fs::metadata(&canonical).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified.hash(&mut hasher);
    max_px.hash(&mut hasher);
    let output = cache_dir.join(format!("{:016x}-{max_px}.jpg", hasher.finish()));
    if output.is_file() {
        return Some(output);
    }

    // Bound decoding of untrusted received images before `thumbnail()`.
    let mut reader = image::ImageReader::open(&canonical)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::no_limits();
    limits.max_image_width = Some(THUMBNAIL_MAX_DIMENSION_PX);
    limits.max_image_height = Some(THUMBNAIL_MAX_DIMENSION_PX);
    limits.max_alloc = Some(THUMBNAIL_MAX_ALLOC_BYTES);
    reader.limits(limits);
    let thumb = reader.decode().ok()?.thumbnail(max_px, max_px).to_rgba8();

    // JPEG has no alpha: composite transparent pixels onto white.
    let rgb = image::RgbImage::from_fn(thumb.width(), thumb.height(), |x, y| {
        let [r, g, b, a] = thumb.get_pixel(x, y).0;
        let blend =
            |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
        image::Rgb([blend(r), blend(g), blend(b)])
    });

    std::fs::create_dir_all(cache_dir).ok()?;
    // Write under a unique name, then rename, so a concurrent request never
    // serves a half-written file.
    let staging = cache_dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let written = std::fs::File::create(&staging)
        .map_err(|e| e.to_string())
        .and_then(|file| {
            let mut writer = std::io::BufWriter::new(file);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, THUMBNAIL_JPEG_QUALITY)
                .encode_image(&rgb)
                .map_err(|e| e.to_string())?;
            writer.flush().map_err(|e| e.to_string())
        })
        .and_then(|()| std::fs::rename(&staging, &output).map_err(|e| e.to_string()));
    if written.is_err() {
        let _ = std::fs::remove_file(&staging);
        // Another request may have produced the same thumbnail meanwhile.
        return output.is_file().then_some(output);
    }
    Some(output)
}

/// Return the filesystem path of a cached JPEG thumbnail (allowed in the asset
/// protocol scope), or `None` for non-images and decode failures.
#[tauri::command]
pub async fn get_thumbnail(
    path: String,
    max_px: Option<u32>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    let max_px = max_px
        .unwrap_or(THUMBNAIL_DEFAULT_PX)
        .clamp(THUMBNAIL_MIN_PX, THUMBNAIL_MAX_PX);
    let Ok(cache_dir) = app.path().app_cache_dir() else {
        return Ok(None);
    };
    let cache_dir = cache_dir.join("thumbnails");
    let thumbnail =
        tokio::task::spawn_blocking(move || cached_thumbnail(&cache_dir, Path::new(&path), max_px))
            .await
            .map_err(|e| e.to_string())?;
    Ok(thumbnail.and_then(|thumbnail| {
        app.asset_protocol_scope()
            .allow_file(&thumbnail)
            .ok()
            .map(|()| thumbnail.to_string_lossy().into_owned())
    }))
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
        let preview = if is_text {
            read_text_preview(p, max)
        } else {
            None
        };

        Ok(match preview {
            Some((content, line_count, truncated)) => FilePreview {
                name,
                size_bytes,
                extension: ext,
                content: Some(content),
                line_count,
                truncated,
            },
            None => FilePreview {
                name,
                size_bytes,
                extension: ext,
                content: None,
                line_count: 0,
                truncated: false,
            },
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Read at most `PREVIEW_MAX_BYTES` and `max_lines` lines of UTF-8 text.
/// Returns `(content, lines in content, truncated)`; `None` if not UTF-8.
/// `truncated` means the file continues past the preview.
fn read_text_preview(path: &Path, max_lines: usize) -> Option<(String, usize, bool)> {
    let file = std::fs::File::open(path).ok()?;
    // One extra byte distinguishes "exactly at the cap" from "more follows".
    let mut reader = std::io::BufReader::new(file.take(PREVIEW_MAX_BYTES as u64 + 1));
    let mut bytes = Vec::new();
    let mut line = Vec::new();
    let mut lines = 0usize;
    let mut truncated = false;
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).ok()? == 0 {
            break;
        }
        if lines == max_lines {
            truncated = true;
            break;
        }
        let room = PREVIEW_MAX_BYTES - bytes.len();
        if line.len() > room {
            bytes.extend_from_slice(&line[..room]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&line);
        lines += 1;
    }

    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        // The byte cap may split a multi-byte character; drop only that tail.
        Err(error) if truncated && error.utf8_error().error_len().is_none() => {
            let valid = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            bytes.truncate(valid);
            String::from_utf8(bytes).ok()?
        }
        Err(_) => return None,
    };
    let preview_lines: Vec<&str> = text.lines().collect();
    Some((preview_lines.join("\n"), preview_lines.len(), truncated))
}

/// Whole-file read returned as a raw IPC body (`ArrayBuffer` in JS), not a
/// JSON number array.
#[tauri::command]
pub async fn read_file_bytes(path: String) -> Result<Response, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        if !p.exists() || !p.is_file() {
            return Err("File not found".to_string());
        }

        std::fs::read(p)
            .map(Response::new)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::{cached_thumbnail, media_scope_path, read_text_preview, PREVIEW_MAX_BYTES};
    use std::path::PathBuf;

    fn test_directory(label: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("landrop-fs-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn thumbnails_are_generated_once_and_keyed_by_size_and_content() {
        let directory = test_directory("thumb");
        let cache = directory.join("cache");
        let source = directory.join("photo.png");
        image::RgbaImage::from_pixel(400, 300, image::Rgba([10, 120, 200, 128]))
            .save(&source)
            .unwrap();

        let first = cached_thumbnail(&cache, &source, 128).expect("thumbnail");
        assert_eq!(first.extension().and_then(|e| e.to_str()), Some("jpg"));
        let decoded = image::open(&first).unwrap();
        assert!(decoded.width() <= 128 && decoded.height() <= 128);
        assert_eq!(decoded.width(), 128, "aspect ratio is preserved");

        // A cache hit must return the existing file without regenerating it.
        std::fs::write(&first, b"cached sentinel").unwrap();
        let second = cached_thumbnail(&cache, &source, 128).expect("cached thumbnail");
        assert_eq!(second, first);
        assert_eq!(std::fs::read(&second).unwrap(), b"cached sentinel");

        // Another size is a different cache entry.
        let larger = cached_thumbnail(&cache, &source, 256).expect("larger thumbnail");
        assert_ne!(larger, first);

        // Replacing the source (new size/mtime) invalidates the entry.
        image::RgbaImage::from_pixel(600, 200, image::Rgba([0, 0, 0, 255]))
            .save(&source)
            .unwrap();
        let replaced = cached_thumbnail(&cache, &source, 128).expect("fresh thumbnail");
        assert_ne!(replaced, first);
        assert_eq!(image::open(&replaced).unwrap().width(), 128);

        // Non-images and undecodable images yield no thumbnail.
        let text = directory.join("notes.txt");
        std::fs::write(&text, b"hello").unwrap();
        assert!(cached_thumbnail(&cache, &text, 128).is_none());
        let fake = directory.join("fake.png");
        std::fs::write(&fake, b"not a png").unwrap();
        assert!(cached_thumbnail(&cache, &fake, 128).is_none());
        assert!(cached_thumbnail(&cache, &directory.join("missing.png"), 128).is_none());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn preview_reads_at_most_the_line_and_byte_caps() {
        let directory = test_directory("preview");

        let many_lines = directory.join("lines.txt");
        let body: String = (0..300).map(|i| format!("line {i}\r\n")).collect();
        std::fs::write(&many_lines, body).unwrap();
        let (content, lines, truncated) = read_text_preview(&many_lines, 200).unwrap();
        assert_eq!(lines, 200);
        assert!(truncated);
        assert!(content.starts_with("line 0\nline 1\n"));
        assert!(content.ends_with("line 199"));

        let short = directory.join("short.txt");
        std::fs::write(&short, "one\ntwo\n").unwrap();
        assert_eq!(
            read_text_preview(&short, 200).unwrap(),
            ("one\ntwo".to_string(), 2, false)
        );

        // One huge line: capped at 256 KiB, cut at a character boundary.
        let huge = directory.join("huge.txt");
        let text = format!("a{}", "é".repeat(150_000));
        std::fs::write(&huge, &text).unwrap();
        let (content, lines, truncated) = read_text_preview(&huge, 200).unwrap();
        assert!(truncated);
        assert_eq!(lines, 1);
        assert_eq!(content.len(), PREVIEW_MAX_BYTES - 1);

        // Exactly at the cap is complete, not truncated.
        let exact = directory.join("exact.txt");
        std::fs::write(&exact, "x".repeat(PREVIEW_MAX_BYTES)).unwrap();
        let (content, _, truncated) = read_text_preview(&exact, 200).unwrap();
        assert_eq!(content.len(), PREVIEW_MAX_BYTES);
        assert!(!truncated);

        let binary = directory.join("blob.bin");
        std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert!(read_text_preview(&binary, 200).is_none());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_existing_image_and_video_files_enter_the_asset_scope() {
        let directory = test_directory("media");
        let photo = directory.join("photo.JPG");
        let clip = directory.join("clip.mp4");
        let doc = directory.join("doc.pdf");
        let folder = directory.join("album.png");
        std::fs::write(&photo, b"x").unwrap();
        std::fs::write(&clip, b"x").unwrap();
        std::fs::write(&doc, b"x").unwrap();
        std::fs::create_dir_all(&folder).unwrap();

        assert_eq!(
            media_scope_path(&photo),
            Some(std::fs::canonicalize(&photo).unwrap())
        );
        assert!(media_scope_path(&clip).is_some());
        assert!(media_scope_path(&doc).is_none());
        assert!(
            media_scope_path(&folder).is_none(),
            "directories are never added"
        );
        assert!(media_scope_path(&directory.join("missing.webp")).is_none());

        #[cfg(unix)]
        {
            let link = directory.join("link.png");
            std::os::unix::fs::symlink(&photo, &link).unwrap();
            assert!(media_scope_path(&link).is_none());
        }

        std::fs::remove_dir_all(directory).unwrap();
    }
}
