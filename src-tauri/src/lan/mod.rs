pub mod discovery;
pub mod identity;
pub mod protocol;
pub mod transfer;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use discovery::DiscoveredPeer;
use identity::normalize_uuid;
use identity::DeviceIdentity;

pub struct LanService {
    pub handle: AppHandle,
    running: Arc<AtomicBool>,
    pub identity: DeviceIdentity,
    data_dir: PathBuf,
    /// All discovered peers on the LAN, keyed by device UUID
    discovered_peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    /// Per-peer output folder overrides, keyed by device UUID
    peer_folders: Arc<Mutex<HashMap<String, String>>>,
    /// Global default output folder
    default_out_folder: Arc<Mutex<String>>,
    /// Current alias (mutable, synced to mDNS)
    alias: Arc<Mutex<String>>,
}

impl LanService {
    pub fn new(handle: AppHandle, identity: DeviceIdentity, data_dir: PathBuf) -> Self {
        let alias = identity.alias.clone();
        Self {
            handle,
            running: Arc::new(AtomicBool::new(false)),
            identity,
            data_dir,
            discovered_peers: Arc::new(Mutex::new(HashMap::new())),
            peer_folders: Arc::new(Mutex::new(HashMap::new())),
            default_out_folder: Arc::new(Mutex::new(String::new())),
            alias: Arc::new(Mutex::new(alias)),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.running.store(true, Ordering::SeqCst);

        let handle = self.handle.clone();
        let running = self.running.clone();
        let identity = self.identity.clone();
        let discovered = self.discovered_peers.clone();
        let folders = self.peer_folders.clone();
        let default_folder = self.default_out_folder.clone();
        let alias = self.alias.clone();

        tokio::spawn(async move {
            discovery::run_discovery(
                handle,
                running,
                identity,
                discovered,
                folders,
                default_folder,
                alias,
            )
            .await;
        });

        Ok(())
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.discovered_peers.lock().await.clear();
    }

    pub async fn send_text(
        &self,
        peer_id: &str,
        peer_ip_hint: Option<&str>,
        text: &str,
    ) -> Result<bool, String> {
        let peer_ip = self.resolve_peer_ip(peer_id, peer_ip_hint).await;
        if let Some(ip) = peer_ip {
            let uuid = self.identity.id_bytes();
            // Retry once on failure (TCP listener may have recovered)
            match transfer::send_text_to_peer(&ip, &uuid, text).await {
                Ok(()) => Ok(true),
                Err(_first_err) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    transfer::send_text_to_peer(&ip, &uuid, text).await?;
                    Ok(true)
                }
            }
        } else {
            Err(format!("Peer {} not found or offline", peer_id))
        }
    }

    pub async fn send_files(
        &self,
        peer_id: &str,
        peer_ip_hint: Option<&str>,
        paths: &[String],
    ) -> Result<bool, String> {
        let peer_ip = self.resolve_peer_ip(peer_id, peer_ip_hint).await;
        if let Some(ip) = peer_ip {
            let uuid = self.identity.id_bytes();
            match transfer::send_files_to_peer(&ip, &uuid, paths, Some(&self.handle)).await {
                Ok(()) => Ok(true),
                Err(_first_err) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    transfer::send_files_to_peer(&ip, &uuid, paths, Some(&self.handle)).await?;
                    Ok(true)
                }
            }
        } else {
            Err(format!("Peer {} not found or offline", peer_id))
        }
    }

    pub async fn set_peer_folder(&self, peer_id: &str, folder: &str) {
        let normalized = normalize_uuid(peer_id).unwrap_or_else(|| peer_id.trim().to_string());
        let mut folders = self.peer_folders.lock().await;
        if folder.is_empty() {
            folders.remove(peer_id);
            folders.remove(&normalized);
        } else {
            folders.insert(normalized, folder.to_string());
        }
    }

    pub async fn set_default_folder(&self, folder: &str) {
        *self.default_out_folder.lock().await = folder.to_string();
    }

    pub async fn set_alias(&mut self, new_alias: &str) {
        *self.alias.lock().await = new_alias.to_string();
        // Update the identity in memory AND persist to disk
        self.identity.alias = new_alias.to_string();
        self.identity.save_alias(&self.data_dir);
    }

    pub fn get_identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    async fn get_peer_ip(&self, peer_id: &str) -> Option<String> {
        let normalized = normalize_uuid(peer_id);
        let peers = self.discovered_peers.lock().await;
        peers
            .get(peer_id)
            .or_else(|| normalized.as_ref().and_then(|id| peers.get(id)))
            .map(|p| p.ip.clone())
    }

    async fn resolve_peer_ip(&self, peer_id: &str, peer_ip_hint: Option<&str>) -> Option<String> {
        if let Some(ip) = self.get_peer_ip(peer_id).await {
            return Some(ip);
        }

        let hinted_ip = peer_ip_hint
            .map(str::trim)
            .filter(|ip| !ip.is_empty())
            .map(str::to_string);

        if hinted_ip.is_some() {
            let _ = self.handle.emit(
                "lan_log",
                serde_json::json!({
                    "level": "warn",
                    "text": format!(
                        "Peer {} missing from backend discovery, using UI IP hint",
                        peer_id
                    ),
                }),
            );
        }

        hinted_ip
    }
}

pub struct LanState {
    pub service: Mutex<LanService>,
}

impl LanState {
    pub fn new(handle: AppHandle, identity: DeviceIdentity, data_dir: PathBuf) -> Self {
        Self {
            service: Mutex::new(LanService::new(handle, identity, data_dir)),
        }
    }
}
