use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

/// Maximum alias length in bytes. The alias is broadcast as one mDNS TXT
/// string (`alias=<value>`), and a single DNS TXT string caps at 255 bytes;
/// 63 keeps the whole record comfortably small (RFC 6763 recommends short keys).
pub const MAX_ALIAS_BYTES: usize = 63;

/// Trim, strip control characters, and cap the alias at MAX_ALIAS_BYTES
/// (on a char boundary) so mDNS registration can never fail on TXT size.
pub(crate) fn sanitize_alias(raw: &str) -> String {
    let mut result = String::new();
    for ch in raw.trim().chars().filter(|ch| !ch.is_control()) {
        if result.len() + ch.len_utf8() > MAX_ALIAS_BYTES {
            break;
        }
        result.push(ch);
    }
    result.trim_end().to_string()
}

pub fn normalize_uuid(id: &str) -> Option<String> {
    uuid::Uuid::parse_str(id.trim())
        .ok()
        .map(|uuid| uuid.to_string())
}

#[derive(Clone, Serialize)]
pub struct DeviceIdentity {
    /// Parsed at load time; `id_bytes` is therefore infallible and two devices
    /// can never silently collide on an all-zero fallback identity.
    pub id: Uuid,
    pub alias: String,
    pub device_type: String,
}

impl DeviceIdentity {
    /// Load existing identity or create a new one on first launch.
    pub fn load_or_create(data_dir: &Path) -> Self {
        let id_file = data_dir.join("device_id.txt");
        let alias_file = data_dir.join("device_alias.txt");
        let make_new_id = || {
            let new_id = Uuid::new_v4();
            let _ = std::fs::write(&id_file, new_id.to_string());
            new_id
        };

        // Ensure data dir exists
        let _ = std::fs::create_dir_all(data_dir);

        // Load or generate UUID (parse once; invalid ids are regenerated)
        let id = match std::fs::read_to_string(&id_file) {
            Ok(s) if !s.trim().is_empty() => match Uuid::parse_str(s.trim()) {
                Ok(parsed) => {
                    let canonical = parsed.to_string();
                    if canonical != s.trim() {
                        let _ = std::fs::write(&id_file, &canonical);
                    }
                    parsed
                }
                Err(_) => make_new_id(),
            },
            _ => make_new_id(),
        };

        // Load or default alias (hostname); always sanitized/size-capped
        let stored_alias = std::fs::read_to_string(&alias_file)
            .map(|s| sanitize_alias(&s))
            .unwrap_or_default();
        let alias = if stored_alias.is_empty() {
            let name = sanitize_alias(
                &hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "My Device".to_string()),
            );
            let name = if name.is_empty() {
                "My Device".to_string()
            } else {
                name
            };
            let _ = std::fs::write(&alias_file, &name);
            name
        } else {
            stored_alias
        };

        let device_type = if cfg!(target_os = "android") {
            "mobile"
        } else {
            "desktop"
        }
        .to_string();

        Self {
            id,
            alias,
            device_type,
        }
    }

    /// Return the 16 raw UUID bytes for the TCP handshake.
    pub fn id_bytes(&self) -> [u8; 16] {
        *self.id.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_uuid, sanitize_alias, MAX_ALIAS_BYTES};

    #[test]
    fn sanitizes_and_caps_alias() {
        assert_eq!(sanitize_alias("  My\tPC\n "), "MyPC");
        assert_eq!(sanitize_alias(""), "");
        let long = "a".repeat(200);
        assert_eq!(sanitize_alias(&long).len(), MAX_ALIAS_BYTES);
        // Multi-byte chars are cut on a char boundary, never mid-encoding
        let emoji = "x".repeat(62) + "🦀🦀";
        let capped = sanitize_alias(&emoji);
        assert!(capped.len() <= MAX_ALIAS_BYTES);
        assert!(capped.ends_with('x'));
    }

    #[test]
    fn normalizes_uuid_variants_to_canonical_format() {
        let canonical = "550e8400-e29b-41d4-a716-446655440000";

        assert_eq!(
            normalize_uuid("550E8400-E29B-41D4-A716-446655440000").as_deref(),
            Some(canonical)
        );
        assert_eq!(
            normalize_uuid("550E8400E29B41D4A716446655440000").as_deref(),
            Some(canonical)
        );
    }
}
