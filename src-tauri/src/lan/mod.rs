pub mod discovery;
pub mod identity;
pub mod protocol;
pub mod tailscale;
pub mod transfer;
#[cfg(target_os = "windows")]
pub mod windows_tailnet;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, Mutex};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

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

/// One discovery run: its cancellation token and the task driving it.
struct DiscoveryRun {
    cancel: CancellationToken,
    join: JoinHandle<()>,
}

pub struct LanService {
    pub handle: AppHandle,
    run: Mutex<Option<DiscoveryRun>>,
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
    /// Current alias; discovery watches this and re-registers mDNS on change
    alias_tx: watch::Sender<String>,
}

impl LanService {
    pub fn new(handle: AppHandle, identity: DeviceIdentity, data_dir: PathBuf) -> Self {
        let alias = identity.alias.clone();
        let folder_settings = load_folder_settings(&data_dir);
        Self {
            handle,
            run: Mutex::new(None),
            identity,
            data_dir,
            discovered_peers: Arc::new(Mutex::new(HashMap::new())),
            peer_folders: Arc::new(Mutex::new(folder_settings.peer_folders)),
            default_out_folder: Arc::new(Mutex::new(folder_settings.default_out_folder)),
            sort_by_date: Arc::new(Mutex::new(folder_settings.sort_by_date)),
            alias_tx: watch::Sender::new(alias),
        }
    }

    pub async fn start(&self) -> Result<(), String> {
        let mut run = self.run.lock().await;
        if let Some(active) = run.as_ref() {
            if !active.cancel.is_cancelled() && !active.join.is_finished() {
                return Ok(());
            }
        }
        // Await the previous run before spawning again: its mDNS goodbye is
        // sent and its TCP listener released, so a fresh registration cannot
        // race a zombie daemon (no fixed sleep needed).
        if let Some(previous) = run.take() {
            previous.cancel.cancel();
            let _ = previous.join.await;
        }

        let cancel = CancellationToken::new();
        let handle = self.handle.clone();
        let identity = self.identity.clone();
        let discovered = self.discovered_peers.clone();
        let receive_routing = discovery::ReceiveRoutingState {
            peer_folders: self.peer_folders.clone(),
            default_out_folder: self.default_out_folder.clone(),
            sort_by_date: self.sort_by_date.clone(),
        };
        let alias_rx = self.alias_tx.subscribe();
        let task_cancel = cancel.clone();

        let join = tokio::spawn(async move {
            discovery::run_discovery(
                handle,
                task_cancel,
                identity,
                discovered,
                receive_routing,
                alias_rx,
            )
            .await;
        });
        *run = Some(DiscoveryRun { cancel, join });

        Ok(())
    }

    pub async fn stop(&self) {
        let mut run = self.run.lock().await;
        if let Some(active) = run.take() {
            active.cancel.cancel();
            let _ = active.join.await;
        }
        let mut peers = self.discovered_peers.lock().await;
        for id in peers.keys() {
            let _ = self
                .handle
                .emit("lan_peer_lost", serde_json::json!({"id": id}));
        }
        peers.clear();
    }

    pub async fn send_text(
        &self,
        peer_id: &str,
        peer_ip_hint: Option<&str>,
        text: &str,
    ) -> Result<bool, String> {
        let connection = self.resolve_available_peer(peer_id, peer_ip_hint).await?;
        // Route fallback happens before any payload. Once sending starts, an
        // error is ambiguous; automatic retries could deliver duplicates.
        transfer::send_text_on_connection(&connection, text).await?;
        Ok(true)
    }

    pub async fn send_files(
        &self,
        peer_id: &str,
        peer_ip_hint: Option<&str>,
        paths: &[String],
    ) -> Result<bool, String> {
        peer_uuid_bytes(peer_id)?;
        match transfer::send_files_on_connection(
            paths,
            Some(&self.handle),
            self.resolve_available_peer(peer_id, peer_ip_hint),
        )
        .await
        {
            Ok(()) => Ok(true),
            Err(error) => {
                let _ = self.handle.emit(
                    "lan_transfer_progress",
                    serde_json::json!({"direction":"send","phase":"error"}),
                );
                Err(error)
            }
        }
    }

    async fn resolve_available_peer(
        &self,
        peer_id: &str,
        hint: Option<&str>,
    ) -> Result<Arc<transfer::Connection>, String> {
        let expected = normalize_uuid(peer_id).ok_or("Invalid peer UUID")?;
        let expected_uuid = peer_uuid_bytes(&expected)?;
        let ips = self.resolve_peer_ips(&expected, hint).await;
        for ip in ips {
            let checked_at = std::time::Instant::now();
            if let Ok(connection) =
                transfer::connect_to_peer(&ip, &self.identity.id_bytes(), &expected_uuid).await
            {
                self.remember_peer_ip(&expected, &ip).await;
                return Ok(connection);
            }
            let mut peers = self.discovered_peers.lock().await;
            if let Some(peer) = peers.get_mut(&expected) {
                peer.forget_route_before(&ip, checked_at);
                if peer.ip.is_empty() {
                    peers.remove(&expected);
                    let _ = self
                        .handle
                        .emit("lan_peer_lost", serde_json::json!({"id":expected}));
                } else {
                    let _ = self.handle.emit("lan_peer_discovered", peer.clone());
                }
            }
        }
        let ip = self.find_peer_on_lan(&expected).await.ok_or_else(|| {
            format!("Peer {peer_id} is offline or unreachable on LAN and Tailscale")
        })?;
        transfer::connect_to_peer(&ip, &self.identity.id_bytes(), &expected_uuid).await
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

    /// Apply a sanitized, size-capped alias, persist it, and notify the
    /// running discovery loop so it re-registers mDNS with the new name.
    /// Returns the alias that was actually applied.
    pub async fn set_alias(&self, new_alias: &str) -> String {
        let sanitized = identity::sanitize_alias(new_alias);
        if sanitized.is_empty() {
            return self.alias_tx.borrow().clone();
        }
        let alias_file = self.data_dir.join("device_alias.txt");
        let _ = fs::write(alias_file, &sanitized);
        self.alias_tx.send_replace(sanitized.clone());
        sanitized
    }

    pub async fn get_identity(&self) -> DeviceIdentity {
        let mut identity = self.identity.clone();
        identity.alias = self.alias_tx.borrow().clone();
        identity
    }

    async fn resolve_peer_ips(&self, peer_id: &str, peer_ip_hint: Option<&str>) -> Vec<String> {
        let normalized = normalize_uuid(peer_id).unwrap_or_else(|| peer_id.to_string());
        let peers = self.discovered_peers.lock().await;
        let mut ips = peers
            .get(&normalized)
            .map(DiscoveredPeer::route_ips)
            .unwrap_or_default();
        // Tailnet routes originate only from the platform inventory and
        // successful app probes. An arbitrary UI hint cannot authorize one.
        if let Some(ip) = peer_ip_hint
            .map(str::trim)
            .filter(|ip| discovery::is_current_lan_peer_ip(ip))
        {
            if !ips.iter().any(|existing| existing == ip) {
                ips.insert(0, ip.to_string());
            }
        }
        ips.retain(|ip| {
            if discovery::is_current_lan_peer_ip(ip) {
                return true;
            }
            let Ok(ip) = ip.parse() else {
                return false;
            };
            if !tailscale::is_tailscale_ipv4(ip) {
                return false;
            }
            #[cfg(target_os = "windows")]
            {
                windows_tailnet::binding_for(ip).is_some()
            }
            #[cfg(not(target_os = "windows"))]
            {
                true
            }
        });
        ips.sort_by_key(|ip| ip.parse().is_ok_and(tailscale::is_tailscale_ipv4));
        ips
    }

    async fn remember_peer_ip(&self, peer_id: &str, ip: &str) {
        let normalized = normalize_uuid(peer_id).unwrap_or_else(|| peer_id.to_string());
        let mut peers = self.discovered_peers.lock().await;
        #[cfg(target_os = "windows")]
        if ip.parse().is_ok_and(|ip| {
            tailscale::is_tailscale_ipv4(ip) && windows_tailnet::binding_for(ip).is_none()
        }) {
            return;
        }
        if let Some(peer) = peers.get_mut(&normalized) {
            peer.observe_route(ip);
            let _ = self.handle.emit("lan_peer_discovered", peer.clone());
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

        let mut candidates = (1..=254).filter(|host| *host != own_host);
        loop {
            while probes.len() < 16 {
                let Some(host) = candidates.next() else {
                    break;
                };
                let ip = Ipv4Addr::new(a, b, c, host).to_string();
                let uuid = my_uuid;
                probes.spawn(async move {
                    transfer::probe_peer_id(&ip, &uuid)
                        .await
                        .ok()
                        .map(|found_id| (found_id, ip))
                });
            }
            let Some(result) = probes.join_next().await else {
                break;
            };
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
