pub mod discovery;
pub mod protocol;
pub mod transfer;

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter};

pub struct LanPeer {
    pub handle: AppHandle,
    running: Arc<Mutex<bool>>,
    connection: Arc<Mutex<Option<Arc<transfer::Connection>>>>,
    code_hash: Arc<Mutex<Vec<u8>>>,
    out_folder: Arc<Mutex<String>>,
}

impl LanPeer {
    pub fn new(handle: AppHandle) -> Self {
        Self {
            handle,
            running: Arc::new(Mutex::new(false)),
            connection: Arc::new(Mutex::new(None)),
            code_hash: Arc::new(Mutex::new(Vec::new())),
            out_folder: Arc::new(Mutex::new(String::new())),
        }
    }

    pub async fn start(&self, code: &str, out_folder: &str) -> Result<(), String> {
        use sha2::{Sha256, Digest};

        let mut running = self.running.lock().await;
        if *running {
            return Ok(());
        }

        let hash = Sha256::digest(code.as_bytes())[..16].to_vec();
        *self.code_hash.lock().await = hash.clone();
        *self.out_folder.lock().await = out_folder.to_string();
        *running = true;
        drop(running);

        let handle = self.handle.clone();
        let running = self.running.clone();
        let connection = self.connection.clone();
        let code_hash = self.code_hash.clone();
        let out_folder_arc = self.out_folder.clone();

        // Spawn discovery + connection loop
        tokio::spawn(async move {
            discovery::run_discovery(handle, running, connection, code_hash, out_folder_arc).await;
        });

        Ok(())
    }

    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
        drop(running);

        let mut conn = self.connection.lock().await;
        *conn = None;
    }

    pub async fn send_text(&self, text: &str) -> Result<bool, String> {
        // Clone the Arc so we don't hold the outer lock during send
        let conn = {
            let guard = self.connection.lock().await;
            guard.as_ref().cloned()
        };
        if let Some(c) = conn {
            match c.send_message(&protocol::Message::Text { text: text.to_string() }).await {
                Ok(_) => Ok(true),
                Err(e) => {
                    *self.connection.lock().await = None;
                    let _ = self.handle.emit("lan_disconnected", serde_json::json!({}));
                    Err(e)
                }
            }
        } else {
            Ok(false)
        }
    }

    pub async fn send_files(&self, paths: &[String]) -> Result<bool, String> {
        // Clone the Arc so we don't hold the outer lock during send
        let conn = {
            let guard = self.connection.lock().await;
            guard.as_ref().cloned()
        };
        if let Some(c) = conn {
            match transfer::send_files(&c, paths, Some(&self.handle)).await {
                Ok(_) => Ok(true),
                Err(e) => {
                    // Send failed — connection is likely broken, clean up
                    *self.connection.lock().await = None;
                    let _ = self.handle.emit("lan_disconnected", serde_json::json!({}));
                    Err(e)
                }
            }
        } else {
            Ok(false)
        }
    }

    #[allow(dead_code)]
    pub async fn is_connected(&self) -> bool {
        self.connection.lock().await.is_some()
    }
}

pub struct LanState {
    pub peer: Mutex<LanPeer>,
}

impl LanState {
    pub fn new(handle: AppHandle) -> Self {
        Self {
            peer: Mutex::new(LanPeer::new(handle)),
        }
    }
}
