use std::path::Path;

fn sanitize_component(value: &str) -> String {
    // Replace only what Windows, macOS or Linux cannot store, so names such as
    // "photo #1.jpg", ".gitignore" or "naïve 🎉.png" arrive unchanged.
    let replaced = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    // Windows silently drops trailing dots and spaces; leading spaces are confusing.
    let trimmed = replaced
        .trim_start_matches(' ')
        .trim_end_matches(['.', ' ']);
    if trimmed.chars().all(|ch| matches!(ch, '.' | '_' | ' ')) {
        return String::new();
    }
    if is_windows_reserved_name(trimmed) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// `CON`, `nul.txt`, `COM1.log` … cannot be created on Windows, whatever the extension.
fn is_windows_reserved_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or("").trim_end();
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.len() == 4
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0')
}

pub(crate) fn sanitize_file_name(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    let file_name = normalized
        .split('/')
        .rfind(|part| !part.is_empty())
        .unwrap_or("file");
    let cleaned = sanitize_component(file_name);
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

pub(crate) fn sanitize_relative_path(name: &str) -> String {
    let mut normalized = name.replace('\\', "/");
    // Strip a Windows drive prefix only when it really is one: two ASCII bytes,
    // so slicing at 2 is always a char boundary. Remote names such as
    // "🎉:party.png" must not reach a byte slice inside a multi-byte character.
    let bytes = normalized.as_bytes();
    if bytes.len() > 1 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        normalized = normalized[2..].to_string();
    }

    let parts = normalized
        .trim_start_matches('/')
        .trim_start_matches('~')
        .trim_start_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .filter_map(|part| {
            let cleaned = sanitize_component(part);
            (!cleaned.is_empty()).then_some(cleaned)
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        sanitize_file_name(
            Path::new(name)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("file"),
        )
    } else {
        parts.join("/")
    }
}

/// Replace `path` with `contents` so a crash or power loss leaves either the old
/// file or the new one, never a torn mix that parses as defaults.
pub(crate) fn write_atomic(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings");
    let staging = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&staging, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{sanitize_file_name, sanitize_relative_path, write_atomic};

    #[test]
    fn atomic_write_replaces_existing_content() {
        let dir = std::env::temp_dir().join(format!("landrop-atomic-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("settings.json");
        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        let leftovers = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(leftovers, 1, "staging file must not remain");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sanitizes_single_file_names() {
        assert_eq!(sanitize_file_name(r"C:\tmp\bad:name?.txt"), "bad_name_.txt");
        assert_eq!(sanitize_file_name("..."), "file");
    }

    #[test]
    fn sanitizes_hostile_relative_transfer_paths() {
        // Drive-relative Windows paths lose the drive prefix.
        assert_eq!(
            sanitize_relative_path(r"C:temp\notes.txt"),
            "temp/notes.txt"
        );
        // UNC paths keep only the sanitized components.
        assert_eq!(
            sanitize_relative_path(r"\\server\share\file.txt"),
            "server/share/file.txt"
        );
        // Trailing dots/spaces (invalid on Windows) are trimmed away.
        assert_eq!(sanitize_relative_path("folder/name..."), "folder/name");
        assert_eq!(sanitize_relative_path("folder/name . "), "folder/name");
        // A component that sanitizes to nothing is dropped, not left empty.
        assert_eq!(sanitize_relative_path("a/.../b.txt"), "a/b.txt");
        // Home-relative and absolute prefixes never escape.
        assert_eq!(sanitize_relative_path("~/secret.txt"), "secret.txt");
        assert_eq!(sanitize_relative_path("/etc/passwd"), "etc/passwd");
        // Long components survive (byte length is bounded by the frame limit).
        let long = "x".repeat(300);
        assert_eq!(sanitize_relative_path(&long), long);
        // Everything stripped: falls back to a safe file name.
        assert_eq!(sanitize_relative_path("../../"), "file");
    }

    #[test]
    fn keeps_ordinary_names_intact() {
        assert_eq!(
            sanitize_file_name("photo #1 (copy).jpg"),
            "photo #1 (copy).jpg"
        );
        assert_eq!(sanitize_file_name(".gitignore"), ".gitignore");
        assert_eq!(
            sanitize_file_name("it's & more+, @home.txt"),
            "it's & more+, @home.txt"
        );
        assert_eq!(sanitize_file_name("naïve 🎉.png"), "naïve 🎉.png");
        assert_eq!(sanitize_file_name("tab\there.txt"), "tab_here.txt");
    }

    #[test]
    fn renames_windows_reserved_names() {
        assert_eq!(sanitize_file_name("CON"), "_CON");
        assert_eq!(sanitize_file_name("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_relative_path("com1/aux.log"), "_com1/_aux.log");
        assert_eq!(sanitize_file_name("COM0.txt"), "COM0.txt");
        assert_eq!(sanitize_file_name("CONSOLE.txt"), "CONSOLE.txt");
    }

    #[test]
    fn sanitizes_relative_transfer_paths() {
        assert_eq!(
            sanitize_relative_path(r"C:\drop\folder\a:b.txt"),
            "drop/folder/a_b.txt"
        );
        assert_eq!(sanitize_relative_path("../../secret.txt"), "secret.txt");
        // Non-ASCII names with a colon must neither panic nor lose their prefix.
        assert_eq!(sanitize_relative_path("🎉:party.png"), "🎉_party.png");
        assert_eq!(sanitize_relative_path("中:a.txt"), "中_a.txt");
        assert_eq!(sanitize_relative_path("é:a.txt"), "é_a.txt");
        assert_eq!(sanitize_relative_path("1:a.txt"), "1_a.txt");
        assert_eq!(
            sanitize_relative_path("/folder/../item.png"),
            "folder/item.png"
        );
    }
}
