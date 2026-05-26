use std::path::Path;

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_' | ' ' | '(' | ')' | '[' | ']') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(&['.', ' ', '_'][..])
        .to_string()
}

pub(crate) fn sanitize_file_name(name: &str) -> String {
    let normalized = name.replace('\\', "/");
    let file_name = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .next_back()
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
    if normalized.len() > 1 && normalized.chars().nth(1) == Some(':') {
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

#[cfg(test)]
mod tests {
    use super::{sanitize_file_name, sanitize_relative_path};

    #[test]
    fn sanitizes_single_file_names() {
        assert_eq!(sanitize_file_name(r"C:\tmp\bad:name?.txt"), "bad_name_.txt");
        assert_eq!(sanitize_file_name("..."), "file");
    }

    #[test]
    fn sanitizes_relative_transfer_paths() {
        assert_eq!(
            sanitize_relative_path(r"C:\drop\folder\a:b.txt"),
            "drop/folder/a_b.txt"
        );
        assert_eq!(sanitize_relative_path("../../secret.txt"), "secret.txt");
        assert_eq!(
            sanitize_relative_path("/folder/../item.png"),
            "folder/item.png"
        );
    }
}
