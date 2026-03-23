use std::path::Path;
use serde::Serialize;

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

        // Ensure data dir exists
        let _ = std::fs::create_dir_all(data_dir);

        // Load or generate UUID
        let id = match std::fs::read_to_string(&id_file) {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => {
                let new_id = uuid::Uuid::new_v4().to_string();
                let _ = std::fs::write(&id_file, &new_id);
                new_id
            }
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

        Self {
            id,
            alias,
            device_type: "desktop".to_string(),
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
