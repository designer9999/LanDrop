pub mod discovery;
pub mod identity;
pub mod protocol;
pub mod transfer;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
use tauri::AppHandle;

use identity::DeviceIdentity;
use discovery::DiscoveredPeer;

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
                handle, running, identity, discovered, folders, default_folder, alias,
            ).await;
        });

        Ok(())
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.discovered_peers.lock().await.clear();
    }

    pub async fn send_text(&self, peer_id: &str, text: &str) -> Result<bool, String> {
        let peer_ip = self.get_peer_ip(peer_id).await;
        if let Some(ip) = peer_ip {
            let uuid = self.identity.id_bytes();
            transfer::send_text_to_peer(&ip, &uuid, text).await?;
            Ok(true)
        } else {
            Err(format!("Peer {} not found or offline", peer_id))
        }
    }

    pub async fn send_files(&self, peer_id: &str, paths: &[String]) -> Result<bool, String> {
        let peer_ip = self.get_peer_ip(peer_id).await;
        if let Some(ip) = peer_ip {
            let uuid = self.identity.id_bytes();
            transfer::send_files_to_peer(&ip, &uuid, paths, Some(&self.handle)).await?;
            Ok(true)
        } else {
            Err(format!("Peer {} not found or offline", peer_id))
        }
    }

    pub async fn set_peer_folder(&self, peer_id: &str, folder: &str) {
        let mut folders = self.peer_folders.lock().await;
        if folder.is_empty() {
            folders.remove(peer_id);
        } else {
            folders.insert(peer_id.to_string(), folder.to_string());
        }
    }

    pub async fn set_default_folder(&self, folder: &str) {
        *self.default_out_folder.lock().await = folder.to_string();
    }

    pub async fn set_alias(&self, new_alias: &str) {
        *self.alias.lock().await = new_alias.to_string();
        // Persist to disk
        let mut identity = self.identity.clone();
        identity.alias = new_alias.to_string();
        identity.save_alias(&self.data_dir);
    }

    pub fn get_identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    async fn get_peer_ip(&self, peer_id: &str) -> Option<String> {
        let peers = self.discovered_peers.lock().await;
        peers.get(peer_id).map(|p| p.ip.clone())
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
