use serde::Serialize;
use std::path::Path;

pub fn normalize_uuid(id: &str) -> Option<String> {
    uuid::Uuid::parse_str(id.trim())
        .ok()
        .map(|uuid| uuid.to_string())
}

#[derive(Clone, Serialize)]
pub struct DeviceIdentity {
    pub id: String,
    pub alias: String,
    pub device_type: String,
}

impl DeviceIdentity {
    /// Load existing identity or create a new one on first launch.
    pub fn load_or_create(data_dir: &Path) -> Self {
        let id_file = data_dir.join("device_id.txt");
        let alias_file = data_dir.join("device_alias.txt");
        let make_new_id = || {
            let new_id = uuid::Uuid::new_v4().to_string();
            let _ = std::fs::write(&id_file, &new_id);
            new_id
        };

        // Ensure data dir exists
        let _ = std::fs::create_dir_all(data_dir);

        // Load or generate UUID
        let id = match std::fs::read_to_string(&id_file) {
            Ok(s) if !s.trim().is_empty() => match normalize_uuid(s.trim()) {
                Some(normalized) => {
                    if normalized != s.trim() {
                        let _ = std::fs::write(&id_file, &normalized);
                    }
                    normalized
                }
                None => make_new_id(),
            },
            _ => make_new_id(),
        };

        // Load or default alias (hostname)
        let alias = match std::fs::read_to_string(&alias_file) {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => {
                let name = hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "My Device".to_string());
                let _ = std::fs::write(&alias_file, &name);
                name
            }
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

    pub fn save_alias(&self, data_dir: &Path) {
        let alias_file = data_dir.join("device_alias.txt");
        let _ = std::fs::write(alias_file, &self.alias);
    }

    /// Return the first 16 bytes of the UUID (as raw bytes) for TCP handshake.
    pub fn id_bytes(&self) -> [u8; 16] {
        match uuid::Uuid::parse_str(&self.id) {
            Ok(u) => *u.as_bytes(),
            Err(_) => [0u8; 16],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_uuid;

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
