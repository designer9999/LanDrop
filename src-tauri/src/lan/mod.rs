pub mod discovery;
pub mod identity;
pub mod protocol;
pub mod transfer;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use discovery::DiscoveredPeer;
use identity::normalize_uuid;
use identity::DeviceIdentity;

fn peer_uuid_bytes(peer_id: &str) -> Result<[u8; 16], String> {
    uuid::Uuid::parse_str(peer_id.trim())
        .map(|uuid| *uuid.as_bytes())
        .map_err(|_| format!("Invalid peer ID: {peer_id}"))
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedFolderSettings {
    default_out_folder: String,
    peer_folders: HashMap<String, String>,
    sort_by_date: bool,
}

fn folder_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("receive_folders.json")
}

fn load_folder_settings(data_dir: &Path) -> PersistedFolderSettings {
    let settings_path = folder_settings_path(data_dir);
    fs::read_to_string(settings_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<PersistedFolderSettings>(&raw).ok())
        .unwrap_or_default()
}

fn save_folder_settings(
    data_dir: &Path,
    default_out_folder: &str,
    peer_folders: &HashMap<String, String>,
    sort_by_date: bool,
) {
    let _ = fs::create_dir_all(data_dir);
    let settings_path = folder_settings_path(data_dir);
    let payload = PersistedFolderSettings {
        default_out_folder: default_out_folder.to_string(),
        peer_folders: peer_folders.clone(),
        sort_by_date,
    };

    if let Ok(json) = serde_json::to_string_pretty(&payload) {
        let _ = fs::write(settings_path, json);
    }
}

pub struct LanService {
    pub handle: AppHandle,
    running: Arc<AtomicBool>,
    identity: DeviceIdentity,
    data_dir: PathBuf,
    /// All discovered peers on the LAN, keyed by device UUID
    discovered_peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    /// Per-peer output folder overrides, keyed by device UUID
    peer_folders: Arc<Mutex<HashMap<String, String>>>,
    /// Global default output folder
    default_out_folder: Arc<Mutex<String>>,
    /// Whether incoming files should be placed into a date-based subfolder
    sort_by_date: Arc<Mutex<bool>>,
    /// Current alias (mutable, synced to mDNS)
    alias: Arc<Mutex<String>>,
}

impl LanService {
    pub fn new(handle: AppHandle, identity: DeviceIdentity, data_dir: PathBuf) -> Self {
        let alias = identity.alias.clone();
        let folder_settings = load_folder_settings(&data_dir);
        Self {
            handle,
            running: Arc::new(AtomicBool::new(false)),
            identity,
            data_dir,
            discovered_peers: Arc::new(Mutex::new(HashMap::new())),
            peer_folders: Arc::new(Mutex::new(folder_settings.peer_folders)),
            default_out_folder: Arc::new(Mutex::new(folder_settings.default_out_folder)),
            sort_by_date: Arc::new(Mutex::new(folder_settings.sort_by_date)),
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
        let receive_routing = discovery::ReceiveRoutingState {
            peer_folders: self.peer_folders.clone(),
            default_out_folder: self.default_out_folder.clone(),
            sort_by_date: self.sort_by_date.clone(),
        };
        let alias = self.alias.clone();

        tokio::spawn(async move {
            discovery::run_discovery(
                handle,
                running,
                identity,
                discovered,
                receive_routing,
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
        let expected_peer_uuid = peer_uuid_bytes(peer_id)?;
        let mut peer_ips = self.resolve_peer_ips(peer_id, peer_ip_hint).await;
        if peer_ips.is_empty() {
            if let Some(ip) = self.find_peer_on_lan(peer_id).await {
                peer_ips.push(ip);
            }
        }
        if peer_ips.is_empty() {
            return Err(format!("Peer {} not found or offline", peer_id));
        }

        let uuid = self.identity.id_bytes();
        let mut last_err = String::new();

        for ip in &peer_ips {
            // Retry once on failure (TCP listener may have recovered)
            for attempt in 0..2 {
                match transfer::send_text_to_peer(ip, &uuid, &expected_peer_uuid, text).await {
                    Ok(()) => {
                        self.remember_peer_ip(peer_id, ip).await;
                        return Ok(true);
                    }
                    Err(err) => {
                        last_err = err;
                        if attempt == 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                }
            }

            let _ = self.handle.emit(
                "lan_log",
                serde_json::json!({
                    "level": "warn",
                    "text": format!("Text send to {} via {} failed: {}", peer_id, ip, last_err),
                }),
            );
        }

        if let Some(ip) = self.find_peer_on_lan(peer_id).await {
            if !peer_ips.iter().any(|existing| existing == &ip) {
                match transfer::send_text_to_peer(&ip, &uuid, &expected_peer_uuid, text).await {
                    Ok(()) => {
                        self.remember_peer_ip(peer_id, &ip).await;
                        return Ok(true);
                    }
                    Err(err) => last_err = err,
                }
            }
        }

        Err(format!(
            "Failed to send to peer {} via {}: {}",
            peer_id,
            peer_ips.join(", "),
            last_err
        ))
    }

    pub async fn send_files(
        &self,
        peer_id: &str,
        peer_ip_hint: Option<&str>,
        paths: &[String],
    ) -> Result<bool, String> {
        let expected_peer_uuid = peer_uuid_bytes(peer_id)?;
        let mut peer_ips = self.resolve_peer_ips(peer_id, peer_ip_hint).await;
        if peer_ips.is_empty() {
            if let Some(ip) = self.find_peer_on_lan(peer_id).await {
                peer_ips.push(ip);
            }
        }
        if peer_ips.is_empty() {
            return Err(format!("Peer {} not found or offline", peer_id));
        }

        let uuid = self.identity.id_bytes();
        let mut last_err = String::new();

        for ip in &peer_ips {
            match transfer::send_files_to_peer(
                ip,
                &uuid,
                &expected_peer_uuid,
                paths,
                Some(&self.handle),
            )
            .await
            {
                Ok(()) => {
                    self.remember_peer_ip(peer_id, ip).await;
                    return Ok(true);
                }
                Err(err) => {
                    last_err = err;
                }
            }

            let _ = self.handle.emit(
                "lan_log",
                serde_json::json!({
                    "level": "warn",
                    "text": format!("File send to {} via {} failed: {}", peer_id, ip, last_err),
                }),
            );
        }

        if let Some(ip) = self.find_peer_on_lan(peer_id).await {
            if !peer_ips.iter().any(|existing| existing == &ip) {
                match transfer::send_files_to_peer(
                    &ip,
                    &uuid,
                    &expected_peer_uuid,
                    paths,
                    Some(&self.handle),
                )
                .await
                {
                    Ok(()) => {
                        self.remember_peer_ip(peer_id, &ip).await;
                        return Ok(true);
                    }
                    Err(err) => last_err = err,
                }
            }
        }

        let _ = self.handle.emit(
            "lan_transfer_progress",
            serde_json::json!({
                "direction": "send",
                "phase": "error",
            }),
        );
        Err(format!(
            "Failed to send to peer {} via {}: {}",
            peer_id,
            peer_ips.join(", "),
            last_err
        ))
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
        drop(folders);
        self.persist_folder_settings().await;
    }

    pub async fn set_default_folder(&self, folder: &str) {
        *self.default_out_folder.lock().await = folder.to_string();
        self.persist_folder_settings().await;
    }

    pub async fn set_sort_by_date(&self, enabled: bool) {
        *self.sort_by_date.lock().await = enabled;
        self.persist_folder_settings().await;
    }

    pub async fn get_folder_settings(&self) -> (String, HashMap<String, String>, bool) {
        let default_out_folder = self.default_out_folder.lock().await.clone();
        let peer_folders = self.peer_folders.lock().await.clone();
        let sort_by_date = *self.sort_by_date.lock().await;
        (default_out_folder, peer_folders, sort_by_date)
    }

    pub async fn set_alias(&self, new_alias: &str) {
        *self.alias.lock().await = new_alias.to_string();
        let alias_file = self.data_dir.join("device_alias.txt");
        let _ = fs::write(alias_file, new_alias);
    }

    pub async fn get_identity(&self) -> DeviceIdentity {
        let mut identity = self.identity.clone();
        identity.alias = self.alias.lock().await.clone();
        identity
    }

    async fn get_peer_ip(&self, peer_id: &str) -> Option<String> {
        let normalized = normalize_uuid(peer_id);
        let peers = self.discovered_peers.lock().await;
        peers
            .get(peer_id)
            .or_else(|| normalized.as_ref().and_then(|id| peers.get(id)))
            .map(|p| p.ip.clone())
    }

    async fn resolve_peer_ips(&self, peer_id: &str, peer_ip_hint: Option<&str>) -> Vec<String> {
        let backend_ip = self.get_peer_ip(peer_id).await;

        let hinted_ip = peer_ip_hint
            .map(str::trim)
            .filter(|ip| !ip.is_empty())
            .map(str::to_string);

        let mut ips = Vec::new();

        if let Some(ip) = backend_ip {
            ips.push(ip);
        }

        if let Some(ip) = hinted_ip {
            if !ips.iter().any(|existing| existing == &ip) {
                ips.push(ip);
            }
        }

        let before_filter = ips.clone();
        ips.retain(|ip| discovery::is_current_lan_peer_ip(ip));
        for rejected in before_filter
            .iter()
            .filter(|ip| !ips.iter().any(|accepted| accepted == *ip))
        {
            let _ = self.handle.emit(
                "lan_log",
                serde_json::json!({
                    "level": "warn",
                    "text": format!(
                        "Ignoring non-LAN peer IP {} for {}; only same LAN as this PC is allowed",
                        rejected, peer_id
                    ),
                }),
            );
        }

        if ips.len() > 1 {
            let _ = self.handle.emit(
                "lan_log",
                serde_json::json!({
                    "level": "info",
                    "text": format!(
                        "Peer {} has multiple candidate IPs: {}",
                        peer_id,
                        ips.join(", ")
                    ),
                }),
            );
        }

        ips
    }

    async fn remember_peer_ip(&self, peer_id: &str, ip: &str) {
        let normalized = normalize_uuid(peer_id);
        let mut peers = self.discovered_peers.lock().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.ip = ip.to_string();
        } else if let Some(peer) = normalized.as_ref().and_then(|id| peers.get_mut(id)) {
            peer.ip = ip.to_string();
        }
    }

    async fn find_peer_on_lan(&self, peer_id: &str) -> Option<String> {
        let target_id = normalize_uuid(peer_id).unwrap_or_else(|| peer_id.trim().to_string());
        let target_label: String = target_id.chars().take(8).collect();
        let local_ip = discovery::get_local_ipv4()?;
        let my_uuid = self.identity.id_bytes();
        let [a, b, c, own_host] = local_ip.octets();
        let mut probes = JoinSet::new();

        let _ = self.handle.emit(
            "lan_log",
            serde_json::json!({
                "level": "info",
                "text": format!("Scanning {}.{}.{}.0/24 for {}", a, b, c, target_label.as_str()),
            }),
        );

        for host in 1..=254 {
            if host == own_host {
                continue;
            }

            let ip = Ipv4Addr::new(a, b, c, host).to_string();
            let uuid = my_uuid;
            probes.spawn(async move {
                transfer::probe_peer_id(&ip, &uuid)
                    .await
                    .ok()
                    .map(|found_id| (found_id, ip))
            });
        }

        while let Some(result) = probes.join_next().await {
            let Ok(Some((found_id, ip))) = result else {
                continue;
            };
            let found_id = normalize_uuid(&found_id).unwrap_or(found_id);
            if found_id == target_id {
                self.remember_peer_ip(&target_id, &ip).await;
                let _ = self.handle.emit(
                    "lan_log",
                    serde_json::json!({
                        "level": "success",
                        "text": format!("Recovered peer {} at {}", target_label.as_str(), ip),
                    }),
                );
                return Some(ip);
            }
        }

        None
    }

    async fn persist_folder_settings(&self) {
        let default_out_folder = self.default_out_folder.lock().await.clone();
        let peer_folders = self.peer_folders.lock().await.clone();
        let sort_by_date = *self.sort_by_date.lock().await;
        save_folder_settings(
            &self.data_dir,
            &default_out_folder,
            &peer_folders,
            sort_by_date,
        );
    }
}

pub struct LanState {
    pub service: LanService,
}

impl LanState {
    pub fn new(handle: AppHandle, identity: DeviceIdentity, data_dir: PathBuf) -> Self {
        Self {
            service: LanService::new(handle, identity, data_dir),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{load_folder_settings, save_folder_settings};
    use std::collections::HashMap;

    #[test]
    fn folder_settings_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!("landrop-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");

        let mut peer_folders = HashMap::new();
        peer_folders.insert("peer-a".to_string(), "C:\\Transfers\\PeerA".to_string());
        peer_folders.insert("peer-b".to_string(), "D:\\Inbox\\PeerB".to_string());

        save_folder_settings(&temp_dir, "C:\\Transfers", &peer_folders, true);
        let loaded = load_folder_settings(&temp_dir);

        assert_eq!(loaded.default_out_folder, "C:\\Transfers");
        assert_eq!(loaded.peer_folders, peer_folders);
        assert!(loaded.sort_by_date);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
