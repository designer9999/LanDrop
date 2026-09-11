use mdns_sd::{ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time;
use tokio_util::sync::CancellationToken;

use super::identity::{normalize_uuid, DeviceIdentity};
use super::protocol::{MDNS_SERVICE_TYPE, TCP_PORT};
use super::tailscale;
use super::transfer::{probe_peer_id, Connection};

const MAX_INCOMING_SESSIONS: usize = 32;

fn service_instance_name(my_id: &str) -> String {
    format!("LanDrop-{}", &my_id[..8])
}

fn service_fullname(my_id: &str) -> String {
    format!("{}.{}", service_instance_name(my_id), MDNS_SERVICE_TYPE)
}

/// Build and register this device's mDNS service record.
fn register_landrop_service(
    handle: &AppHandle,
    mdns: &ServiceDaemon,
    my_id: &str,
    alias: &str,
    device_type: &str,
    local_ip: Ipv4Addr,
) {
    let properties = [("id", my_id), ("alias", alias), ("dtype", device_type)];
    let host_name = format!("landrop-{}.local.", &my_id[..8]);
    let instance_name = service_instance_name(my_id);

    match ServiceInfo::new(
        MDNS_SERVICE_TYPE,
        &instance_name,
        &host_name,
        local_ip.to_string(),
        TCP_PORT,
        &properties[..],
    ) {
        Ok(service) => match mdns.register(service) {
            Ok(_) => emit_log(
                handle,
                "success",
                &format!("Registered as \"{}\" on {}:{}", alias, local_ip, TCP_PORT),
            ),
            Err(e) => emit_log(
                handle,
                "error",
                &format!("Failed to register mDNS service: {}", e),
            ),
        },
        Err(e) => {
            emit_log(
                handle,
                "error",
                &format!("Failed to create mDNS service info: {}", e),
            );
        }
    }
}

/// Emit a log event to the frontend debug panel
fn emit_log(handle: &AppHandle, level: &str, text: &str) {
    let _ = handle.emit(
        "lan_log",
        serde_json::json!({
            "level": level,
            "text": text,
        }),
    );
    eprintln!("[LAN {}] {}", level, text);
}

fn emit_transfer_error(handle: &AppHandle, direction: &str) {
    let _ = handle.emit(
        "lan_transfer_progress",
        serde_json::json!({
            "direction": direction,
            "phase": "error",
        }),
    );
}

#[cfg(target_os = "android")]
fn emit_android_receive_notification(handle: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;

    let focused = handle
        .get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .is_some_and(|focused| focused);
    if focused {
        return;
    }

    let _ = handle
        .notification()
        .builder()
        .channel_id("landrop-incoming-v2")
        .title(title)
        .body(body)
        .group("landrop")
        .auto_cancel()
        .show();
}

#[cfg(not(target_os = "android"))]
fn emit_android_receive_notification(_handle: &AppHandle, _title: &str, _body: &str) {}

fn notification_text_preview(text: &str) -> String {
    const MAX_CHARS: usize = 160;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "New message received".to_string();
    }

    let mut preview: String = trimmed.chars().take(MAX_CHARS).collect();
    if trimmed.chars().count() > MAX_CHARS {
        preview.push_str("...");
    }
    preview
}

const BAD_INTERFACE_KEYWORDS: &[&str] = &[
    "tailscale",
    "tun",
    "tap",
    "wg",
    "vpn",
    "proton",
    "nord",
    "mullvad",
    "wireguard",
    "virtual",
    "hyper-v",
    "vethernet",
    "vmware",
    "virtualbox",
    "wsl",
    "docker",
    "bluetooth",
    "loopback",
];

fn is_bad_interface(name: &str) -> bool {
    let name_lower = name.to_lowercase();
    BAD_INTERFACE_KEYWORDS
        .iter()
        .any(|keyword| name_lower.contains(keyword))
}

fn is_usable_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(tailscale::is_tailscale_ipv4(ip)
        || ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 169 && octets[1] == 254)
}

fn private_ipv4_score(ip: Ipv4Addr) -> i32 {
    let octets = ip.octets();
    if octets[0] == 192 && octets[1] == 168 {
        30
    } else if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        20
    } else if octets[0] == 10 {
        10
    } else {
        1
    }
}

fn is_same_lan_ipv4(local_ip: Ipv4Addr, peer_ip: Ipv4Addr) -> bool {
    let local = local_ip.octets();
    let peer = peer_ip.octets();
    local[0] == peer[0] && local[1] == peer[1] && local[2] == peer[2]
}

fn local_interface_score(name: &str, ip: Ipv4Addr) -> i32 {
    if !is_usable_ipv4(ip) {
        return -1000;
    }

    let mut score = private_ipv4_score(ip);
    if is_bad_interface(name) {
        score -= 100;
    }
    score
}

fn peer_address_score(local_ip: Ipv4Addr, peer_ip: Ipv4Addr) -> i32 {
    if !is_usable_ipv4(peer_ip) || !is_same_lan_ipv4(local_ip, peer_ip) {
        return -1000;
    }

    let mut score = private_ipv4_score(peer_ip);
    score += 100;

    score
}

/// Get the local LAN IPv4 address, preferring physical private LAN adapters over VPN/VM adapters.
pub fn get_local_ipv4() -> Option<Ipv4Addr> {
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();

    interfaces
        .iter()
        .filter_map(|(name, ip)| match ip {
            IpAddr::V4(v4) => Some((*v4, local_interface_score(name, *v4))),
            _ => None,
        })
        .filter(|(_, score)| *score > 0)
        .max_by_key(|(_, score)| *score)
        .map(|(ip, _)| ip)
        .or_else(|| {
            interfaces.iter().find_map(|(name, ip)| {
                if is_bad_interface(name) {
                    return None;
                }
                match ip {
                    IpAddr::V4(v4) if is_usable_ipv4(*v4) => Some(*v4),
                    _ => None,
                }
            })
        })
        .or_else(|| {
            interfaces.iter().find_map(|(_, ip)| match ip {
                IpAddr::V4(v4) if is_usable_ipv4(*v4) => Some(*v4),
                _ => None,
            })
        })
}

fn choose_peer_ipv4<'a>(
    addresses: impl Iterator<Item = &'a ScopedIp>,
    local_ip: Ipv4Addr,
) -> Option<Ipv4Addr> {
    addresses
        .filter_map(|addr| match addr.to_ip_addr() {
            IpAddr::V4(v4) => Some((v4, peer_address_score(local_ip, v4))),
            _ => None,
        })
        .filter(|(_, score)| *score > 0)
        .max_by_key(|(_, score)| *score)
        .map(|(ip, _)| ip)
}

pub fn is_current_lan_peer_ip(peer_ip: &str) -> bool {
    match (get_local_ipv4(), peer_ip.parse::<Ipv4Addr>()) {
        (Some(local_ip), Ok(peer_ip)) => is_same_lan_ipv4(local_ip, peer_ip),
        _ => false,
    }
}

fn same_lan_probe_ips(local_ip: Ipv4Addr) -> Vec<Ipv4Addr> {
    let [a, b, c, own_host] = local_ip.octets();
    (1..=254)
        .filter(|host| *host != own_host)
        .map(|host| Ipv4Addr::new(a, b, c, host))
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct DiscoveredPeer {
    pub id: String,
    pub alias: String,
    pub device_type: String,
    pub ip: String,
    pub port: u16,
    pub network: String,
    pub lan_ip: Option<String>,
    pub tailscale_ip: Option<String>,
    #[serde(skip)]
    lan_seen: Option<Instant>,
    #[serde(skip)]
    tailscale_seen: Option<Instant>,
}

impl DiscoveredPeer {
    pub fn new(id: String, alias: String, device_type: String, ip: String) -> Self {
        let mut peer = Self {
            id,
            alias,
            device_type,
            ip: String::new(),
            port: TCP_PORT,
            network: String::new(),
            lan_ip: None,
            tailscale_ip: None,
            lan_seen: None,
            tailscale_seen: None,
        };
        peer.observe_route(&ip);
        peer
    }

    pub fn observe_route(&mut self, ip: &str) {
        if ip.parse().is_ok_and(tailscale::is_tailscale_ipv4) {
            self.tailscale_ip = Some(ip.to_string());
            self.tailscale_seen = Some(Instant::now());
        } else {
            self.lan_ip = Some(ip.to_string());
            self.lan_seen = Some(Instant::now());
        }
        self.select_preferred_route();
    }

    fn select_preferred_route(&mut self) {
        if let Some(ip) = &self.lan_ip {
            self.ip = ip.clone();
            self.network = "lan".into();
        } else if let Some(ip) = &self.tailscale_ip {
            self.ip = ip.clone();
            self.network = "tailscale".into();
        } else {
            self.ip.clear();
        }
    }

    pub fn route_ips(&self) -> Vec<String> {
        self.lan_ip
            .iter()
            .chain(self.tailscale_ip.iter())
            .cloned()
            .collect()
    }

    pub fn forget_route_before(&mut self, ip: &str, checked_at: Instant) {
        if self.lan_ip.as_deref() == Some(ip) && self.lan_seen.is_none_or(|seen| seen <= checked_at)
        {
            self.lan_ip = None;
            self.lan_seen = None;
        }
        if self.tailscale_ip.as_deref() == Some(ip)
            && self.tailscale_seen.is_none_or(|seen| seen <= checked_at)
        {
            self.tailscale_ip = None;
            self.tailscale_seen = None;
        }
        self.select_preferred_route();
    }
}

#[derive(Clone)]
pub struct ReceiveRoutingState {
    pub peer_folders: Arc<Mutex<HashMap<String, String>>>,
    pub default_out_folder: Arc<Mutex<String>>,
    pub sort_by_date: Arc<Mutex<bool>>,
}

struct IncomingSessionContext<'a> {
    handle: &'a AppHandle,
    receive_routing: &'a ReceiveRoutingState,
    discovered_peers: &'a Mutex<HashMap<String, DiscoveredPeer>>,
    pending_removals: &'a Mutex<HashMap<String, (Instant, String, u16)>>,
    tailnet_ips: &'a Mutex<HashSet<Ipv4Addr>>,
    alias_rx: &'a tokio::sync::watch::Receiver<String>,
    device_type: &'a str,
    cancel: &'a CancellationToken,
}

/// Run mDNS-based discovery: register this device, browse for others, accept TCP transfers.
pub async fn run_discovery(
    handle: AppHandle,
    cancel: CancellationToken,
    identity: DeviceIdentity,
    discovered_peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    receive_routing: ReceiveRoutingState,
    mut alias_rx: tokio::sync::watch::Receiver<String>,
) {
    // Get our local LAN IP
    let local_ip = get_local_ipv4().unwrap_or(Ipv4Addr::UNSPECIFIED);
    emit_log(&handle, "info", &format!("Local IP: {}", local_ip));

    // Create mDNS daemon
    let mdns = match ServiceDaemon::new() {
        Ok(d) => {
            emit_log(&handle, "success", "mDNS daemon started");
            Some(d)
        }
        Err(e) => {
            let err_msg = format!("Failed to create mDNS daemon: {}", e);
            emit_log(&handle, "error", &err_msg);

            // Linux-specific diagnostics: common causes are firewall blocking
            // UDP 5353 or NetworkManager not enabling mDNS per connection.
            #[cfg(target_os = "linux")]
            {
                emit_log(
                    &handle,
                    "warn",
                    "Linux mDNS troubleshooting: ensure UDP port 5353 is allowed \
                     by your firewall (ufw/firewalld), and that NetworkManager \
                     has connection.mdns=2 (or install avahi-daemon).",
                );
            }
            None
        }
    };

    // Register our service
    let my_id = identity.id.to_string();
    let current_alias = alias_rx.borrow_and_update().clone();
    if let Some(mdns) = mdns.as_ref().filter(|_| !local_ip.is_unspecified()) {
        register_landrop_service(
            &handle,
            mdns,
            &my_id,
            &current_alias,
            &identity.device_type,
            local_ip,
        );
    }

    // Browse for other instances
    let browse_receiver = match mdns.as_ref().map(|mdns| mdns.browse(MDNS_SERVICE_TYPE)) {
        Some(Ok(r)) => {
            emit_log(
                &handle,
                "success",
                "Browsing for LanDrop devices on network...",
            );
            Some(r)
        }
        Some(Err(e)) => {
            emit_log(&handle, "error", &format!("Failed to browse mDNS: {}", e));
            None
        }
        None => None,
    };

    // Bind TCP listener — retry up to 5 times if port is held by previous instance
    let tcp_listener = {
        let mut listener_opt = None;
        for attempt in 0..5 {
            match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, TCP_PORT)).await {
                Ok(l) => {
                    emit_log(
                        &handle,
                        "success",
                        &format!("TCP listener on port {}", TCP_PORT),
                    );
                    listener_opt = Some(Arc::new(l));
                    break;
                }
                Err(e) if attempt < 4 => {
                    emit_log(
                        &handle,
                        "warn",
                        &format!("TCP bind retry {}/5: {}", attempt + 1, e),
                    );
                    time::sleep(Duration::from_millis(800)).await;
                }
                Err(e) => {
                    emit_log(
                        &handle,
                        "error",
                        &format!(
                            "Failed to bind TCP port {} after 5 retries: {}",
                            TCP_PORT, e
                        ),
                    );
                    if let Some(mdns) = &mdns {
                        let _ = mdns.shutdown();
                    }
                    return;
                }
            }
        }
        let Some(listener) = listener_opt else {
            if let Some(mdns) = &mdns {
                let _ = mdns.shutdown();
            }
            return;
        };
        listener
    };

    // ── Task 1: mDNS event processor ──
    // Pending removals: peer_id → (scheduled_at, peer_ip, peer_port)
    // mDNS ServiceRemoved is unreliable on Windows — spurious removals happen
    // on network adapter changes, Wi-Fi blips, mDNS cache expiry, etc.
    // We defer removal and verify with a TCP check before marking offline.
    type PendingRemovalMap = Arc<Mutex<HashMap<String, (Instant, String, u16)>>>;
    let pending_removals: PendingRemovalMap = Arc::new(Mutex::new(HashMap::new()));
    let tailnet_ips = Arc::new(Mutex::new(HashSet::new()));

    let cancel_mdns = cancel.clone();
    let handle_mdns = handle.clone();
    let peers_mdns = discovered_peers.clone();
    let my_id_mdns = my_id.clone();
    let my_uuid_mdns = identity.id_bytes();
    let pending_mdns = pending_removals.clone();
    let local_ip_mdns = local_ip;
    let mdns_task = mdns.clone();
    let device_type_mdns = identity.device_type.clone();
    let mut alias_rx_mdns = alias_rx.clone();
    let mdns_processor = tokio::spawn(async move {
        let grace_period = Duration::from_secs(15);
        let mut sweep_interval = time::interval(Duration::from_secs(5));
        sweep_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
        let mut alias_watch_alive = true;

        loop {
            tokio::select! {
                _ = cancel_mdns.cancelled() => break,

                // ── Alias changed: re-register the service with the new TXT ──
                changed = alias_rx_mdns.changed(), if alias_watch_alive => {
                    if changed.is_err() {
                        alias_watch_alive = false;
                        continue;
                    }
                    let new_alias = alias_rx_mdns.borrow_and_update().clone();
                    let Some(mdns_task) = &mdns_task else { continue; };
                    if let Err(e) = mdns_task.unregister(&service_fullname(&my_id_mdns)) {
                        emit_log(
                            &handle_mdns,
                            "warn",
                            &format!("mDNS unregister before alias update failed: {}", e),
                        );
                    }
                    register_landrop_service(
                        &handle_mdns,
                        mdns_task,
                        &my_id_mdns,
                        &new_alias,
                        &device_type_mdns,
                        local_ip_mdns,
                    );
                }

                // ── Sweep pending removals every 5 seconds ──
                _ = sweep_interval.tick() => {
                    let mut pending = pending_mdns.lock().await;
                    let expired: Vec<(String, String, u16)> = pending
                        .iter()
                        .filter(|(_, (at, _, _))| at.elapsed() >= grace_period)
                        .map(|(id, (_, ip, port))| (id.clone(), ip.clone(), *port))
                        .collect();
                    for (id, _, _) in expired {
                        pending.remove(&id);
                        // The route monitor verifies every peer independently of
                        // mDNS goodbyes, including peers learned by TCP scans.
                    }
                }

                // ── mDNS browse events, natively async (no blocking-pool churn) ──
                event = async {
                    match &browse_receiver {
                        Some(receiver) => receiver.recv_async().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let Ok(event) = event else { break };
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            // The current wire protocol uses one fixed service
                            // port; do not silently route an incompatible record.
                            if info.get_port() != TCP_PORT { continue; }
                            // Extract peer info from TXT records
                            let props = info.get_properties();
                            let raw_peer_id = props.get_property_val_str("id").unwrap_or_default();
                            let peer_id = match normalize_uuid(raw_peer_id) {
                                Some(id) => id,
                                None => {
                                    if !raw_peer_id.is_empty() {
                                        emit_log(
                                            &handle_mdns,
                                            "warn",
                                            &format!(
                                                "Ignoring peer with invalid UUID: {}",
                                                raw_peer_id
                                            ),
                                        );
                                    }
                                    continue;
                                }
                            };
                            let peer_alias = props
                                .get_property_val_str("alias")
                                .unwrap_or_default()
                                .to_string();
                            let peer_dtype = props
                                .get_property_val_str("dtype")
                                .unwrap_or("desktop")
                                .to_string();

                            // Skip our own service
                            if peer_id == my_id_mdns || peer_id.is_empty() {
                                continue;
                            }

                            let ip = match choose_peer_ipv4(info.get_addresses().iter(), local_ip_mdns)
                            {
                                Some(ip) => ip.to_string(),
                                None => {
                                    emit_log(
                                        &handle_mdns,
                                        "warn",
                                        &format!(
                                            "Ignoring peer {} with no usable LAN IPv4 address",
                                            &peer_id[..8]
                                        ),
                                    );
                                    continue;
                                }
                            };

                            let advertised_ipv4s: Vec<String> = info
                                .get_addresses()
                                .iter()
                                .filter_map(|addr| match addr.to_ip_addr() {
                                    IpAddr::V4(v4) => Some(v4.to_string()),
                                    _ => None,
                                })
                                .collect();
                            if advertised_ipv4s.len() > 1 {
                                emit_log(
                                    &handle_mdns,
                                    "info",
                                    &format!(
                                        "Peer {} advertised IPs {}; using {}",
                                        &peer_id[..8],
                                        advertised_ipv4s.join(", "),
                                        ip
                                    ),
                                );
                            }

                            if ip.is_empty() {
                                continue;
                            }

                            // Cancel any pending removal — peer is alive
                            {
                                let mut pending = pending_mdns.lock().await;
                                if pending.remove(&peer_id).is_some() {
                                    emit_log(
                                        &handle_mdns,
                                        "info",
                                        &format!(
                                            "Cancelled pending removal for {} (re-discovered)",
                                            &peer_id[..8]
                                        ),
                                    );
                                }
                            }

                            // A multicast advertisement is only a candidate;
                            // verify its UUID before exposing it as online.
                            if probe_peer_id(&ip, &my_uuid_mdns).await.as_deref() != Ok(peer_id.as_str()) {
                                continue;
                            }
                            let mut peers = peers_mdns.lock().await;
                            let peer = peers.entry(peer_id.clone()).or_insert_with(|| DiscoveredPeer::new(
                                peer_id.clone(), peer_alias.clone(), peer_dtype.clone(), ip.clone()));
                            peer.alias = peer_alias;
                            peer.device_type = peer_dtype;
                            peer.observe_route(&ip);
                            let peer = peer.clone();
                            drop(peers);

                            let _ = handle_mdns.emit("lan_peer_discovered", &peer);
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            // DON'T immediately remove — schedule a pending removal.
                            // mDNS ServiceRemoved is unreliable on Windows.
                            let peers = peers_mdns.lock().await;
                            let found = peers
                                .iter()
                                .find(|(_, p)| fullname.contains(&p.id[..8]))
                                .map(|(id, p)| (id.clone(), p.ip.clone(), p.port));
                            drop(peers);

                            if let Some((id, ip, port)) = found {
                                let mut pending = pending_mdns.lock().await;
                                pending.insert(id.clone(), (Instant::now(), ip, port));
                                emit_log(
                                    &handle_mdns,
                                    "info",
                                    &format!(
                                        "mDNS removal for {} — verifying in {}s...",
                                        &id[..8],
                                        grace_period.as_secs()
                                    ),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    // ── Task 2: TCP listener — accepts incoming transfers ──
    let cancel_tcp = cancel.clone();
    let handle_tcp = handle.clone();
    let my_uuid = identity.id_bytes();
    let peers_tcp = discovered_peers.clone();
    let pending_tcp = pending_removals.clone();
    let tailnet_tcp = tailnet_ips.clone();
    let alias_tcp = alias_rx.clone();
    let device_type_tcp = identity.device_type.clone();
    let incoming_session_slots = Arc::new(Semaphore::new(MAX_INCOMING_SESSIONS));
    let tcp_acceptor = tokio::spawn(async move {
        let mut listener: Arc<TcpListener> = tcp_listener;
        let mut consecutive_errors: u32 = 0;
        let mut last_capacity_warning: Option<Instant> = None;

        loop {
            // If accept keeps failing, the socket is dead (network change, sleep/wake).
            // Rebind the listener to recover.
            if consecutive_errors >= 5 {
                emit_log(&handle_tcp, "warn", "TCP listener unhealthy — rebinding...");
                match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, TCP_PORT)).await {
                    Ok(new_listener) => {
                        listener = Arc::new(new_listener);
                        consecutive_errors = 0;
                        emit_log(&handle_tcp, "success", "TCP listener rebound successfully");
                    }
                    Err(e) => {
                        emit_log(&handle_tcp, "error", &format!("TCP rebind failed: {}", e));
                        tokio::select! {
                            _ = cancel_tcp.cancelled() => break,
                            _ = time::sleep(Duration::from_secs(3)) => {}
                        }
                        continue;
                    }
                }
            }

            let accepted = tokio::select! {
                _ = cancel_tcp.cancelled() => break,
                accepted = listener.accept() => accepted,
            };
            match accepted {
                Ok((stream, _addr)) => {
                    consecutive_errors = 0;
                    let permit = match incoming_session_slots.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            if last_capacity_warning
                                .is_none_or(|at| at.elapsed() >= Duration::from_secs(5))
                            {
                                emit_log(
                                    &handle_tcp,
                                    "warn",
                                    &format!(
                                        "Incoming connection limit reached ({MAX_INCOMING_SESSIONS})"
                                    ),
                                );
                                last_capacity_warning = Some(Instant::now());
                            }
                            drop(stream);
                            continue;
                        }
                    };
                    let handle_session = handle_tcp.clone();
                    let receive_routing = receive_routing.clone();
                    let peers_ref = peers_tcp.clone();
                    let pending_ref = pending_tcp.clone();
                    let tailnet_ref = tailnet_tcp.clone();
                    let alias_ref = alias_tcp.clone();
                    let device_type = device_type_tcp.clone();
                    let cancel_session = cancel_tcp.clone();

                    tokio::spawn(async move {
                        let _permit = permit;
                        let context = IncomingSessionContext {
                            handle: &handle_session,
                            receive_routing: &receive_routing,
                            discovered_peers: &peers_ref,
                            pending_removals: &pending_ref,
                            tailnet_ips: &tailnet_ref,
                            alias_rx: &alias_ref,
                            device_type: &device_type,
                            cancel: &cancel_session,
                        };
                        match handle_incoming_session(stream, &my_uuid, &context).await {
                            Ok(_) => {}
                            Err(e) => {
                                let _ = handle_session.emit(
                                    "lan_log",
                                    serde_json::json!({
                                        "level": "error",
                                        "text": format!("Incoming session error: {}", e),
                                    }),
                                );
                                eprintln!("Incoming session error: {}", e);
                            }
                        }
                    });
                }
                Err(_) => {
                    consecutive_errors += 1;
                }
            }
        }
    });

    // ── Task 3: same-LAN TCP healer scan ──
    // mDNS can be lost or polluted by VPN/virtual interfaces. Probe only this
    // machine's /24 LAN and recover peers by their UUID handshake.
    let cancel_scan = cancel.clone();
    let handle_scan = handle.clone();
    let peers_scan = discovered_peers.clone();
    let my_id_scan = my_id.clone();
    let my_uuid_scan = identity.id_bytes();
    let lan_scanner = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = cancel_scan.cancelled() => break,
                _ = interval.tick() => {}
            }

            let Some(local_ip) = get_local_ipv4() else {
                continue;
            };

            let mut probes = JoinSet::new();
            let mut candidates = same_lan_probe_ips(local_ip).into_iter();
            loop {
                while probes.len() < 16 {
                    let Some(ip) = candidates.next() else {
                        break;
                    };
                    let ip_string = ip.to_string();
                    let uuid = my_uuid_scan;
                    probes.spawn(async move {
                        probe_peer_id(&ip_string, &uuid)
                            .await
                            .ok()
                            .map(|peer_id| (peer_id, ip_string))
                    });
                }
                let result = tokio::select! {
                    _ = cancel_scan.cancelled() => return,
                    result = probes.join_next() => result,
                };
                let Some(result) = result else {
                    break;
                };
                let Ok(Some((raw_peer_id, ip))) = result else {
                    continue;
                };
                let Some(peer_id) = normalize_uuid(&raw_peer_id) else {
                    continue;
                };
                if peer_id == my_id_scan {
                    continue;
                }

                let mut peers = peers_scan.lock().await;
                let changed = peers
                    .get(&peer_id)
                    .is_none_or(|peer| peer.lan_ip.as_deref() != Some(&ip));
                let peer = peers.entry(peer_id.clone()).or_insert_with(|| {
                    DiscoveredPeer::new(
                        peer_id.clone(),
                        format!("Device-{}", &peer_id[..8]),
                        "desktop".into(),
                        ip.clone(),
                    )
                });
                peer.observe_route(&ip);
                let peer = peer.clone();
                drop(peers);

                if changed {
                    emit_log(
                        &handle_scan,
                        "info",
                        &format!("LAN scan found {} at {}", &peer_id[..8], ip),
                    );
                    let _ = handle_scan.emit("lan_peer_discovered", &peer);
                }
            }
        }
    });

    let tailnet_scanner = tokio::spawn(run_tailnet_discovery(
        handle.clone(),
        cancel.clone(),
        identity.id_bytes(),
        my_id.clone(),
        discovered_peers.clone(),
        tailnet_ips,
    ));
    let route_monitor = tokio::spawn(monitor_routes(
        handle.clone(),
        cancel.clone(),
        identity.id_bytes(),
        discovered_peers,
    ));
    let _ = tokio::join!(
        mdns_processor,
        tcp_acceptor,
        lan_scanner,
        tailnet_scanner,
        route_monitor
    );

    // Graceful shutdown — send mDNS goodbye
    if let Some(mdns) = &mdns {
        let _ = mdns.shutdown();
    }
}

async fn run_tailnet_discovery(
    handle: AppHandle,
    cancel: CancellationToken,
    my_uuid: [u8; 16],
    my_id: String,
    peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    tailnet_ips: Arc<Mutex<HashSet<Ipv4Addr>>>,
) {
    let mut interval = time::interval(Duration::from_secs(15));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let mut last_status = String::new();
    loop {
        tokio::select! { _ = cancel.cancelled() => break, _ = interval.tick() => {} }
        let result = tokio::select! {
            _ = cancel.cancelled() => break,
            result = tailscale::online_peers() => result,
        };
        let (ips, state, message) = match result {
            Ok(ips) => (
                ips,
                "available",
                "Tailscale connected; discovering running LanDrop devices".to_string(),
            ),
            Err(error) => (HashSet::new(), "unavailable", error),
        };
        if message != last_status {
            let _ = handle.emit(
                "tailscale_status",
                serde_json::json!({"state": state, "message": message}),
            );
            emit_log(&handle, "info", &message);
            last_status = message;
        }
        *tailnet_ips.lock().await = ips.clone();
        // Bounded concurrency and bounded per-probe time keep large tailnets
        // responsive without opening hundreds of simultaneous sessions.
        let mut candidates = ips.into_iter();
        let mut probes = JoinSet::new();
        loop {
            while probes.len() < 16 {
                let Some(ip) = candidates.next() else {
                    break;
                };
                probes.spawn(async move {
                    let ip = ip.to_string();
                    super::transfer::probe_peer_info(&ip, &my_uuid)
                        .await
                        .ok()
                        .map(|info| (ip, info))
                });
            }
            let result = tokio::select! {
                _ = cancel.cancelled() => return,
                result = probes.join_next() => result,
            };
            let Some(result) = result else {
                break;
            };
            let Ok(Some((ip, (id, alias, dtype)))) = result else {
                continue;
            };
            if id == my_id {
                continue;
            }
            let mut peers = peers.lock().await;
            let peer = peers.entry(id.clone()).or_insert_with(|| {
                DiscoveredPeer::new(id, alias.clone(), dtype.clone(), ip.clone())
            });
            peer.alias = alias;
            peer.device_type = dtype;
            peer.observe_route(&ip);
            let peer = peer.clone();
            drop(peers);
            let _ = handle.emit("lan_peer_discovered", peer);
        }
    }
    let _ = handle.emit(
        "tailscale_status",
        serde_json::json!({"state":"stopped", "message":"Discovery stopped"}),
    );
}

/// TCP-scan-only peers also expire, even if no mDNS goodbye ever arrives.
async fn monitor_routes(
    handle: AppHandle,
    cancel: CancellationToken,
    my_uuid: [u8; 16],
    peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
) {
    let mut interval = time::interval(Duration::from_secs(15));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    loop {
        tokio::select! { _ = cancel.cancelled() => return, _ = interval.tick() => {} }
        let checked_at = Instant::now();
        let snapshot: Vec<_> = peers
            .lock()
            .await
            .values()
            .flat_map(|peer| peer.route_ips().into_iter().map(|ip| (peer.id.clone(), ip)))
            .collect();
        let mut candidates = snapshot.into_iter();
        let mut probes = JoinSet::new();
        loop {
            while probes.len() < 16 {
                let Some((id, ip)) = candidates.next() else {
                    break;
                };
                probes.spawn(async move {
                    let alive = if ip.parse().is_ok_and(tailscale::is_tailscale_ipv4) {
                        super::transfer::probe_peer_info(&ip, &my_uuid)
                            .await
                            .is_ok_and(|(found, _, _)| found == id)
                    } else {
                        probe_peer_id(&ip, &my_uuid)
                            .await
                            .is_ok_and(|found| found == id)
                    };
                    (id, ip, alive)
                });
            }
            let result = tokio::select! { _ = cancel.cancelled() => return, result = probes.join_next() => result };
            let Some(result) = result else {
                break;
            };
            let Ok((id, ip, alive)) = result else {
                continue;
            };
            if alive {
                continue;
            }
            let mut peers = peers.lock().await;
            let Some(peer) = peers.get_mut(&id) else {
                continue;
            };
            peer.forget_route_before(&ip, checked_at);
            if peer.ip.is_empty() {
                peers.remove(&id);
                let _ = handle.emit("lan_peer_lost", serde_json::json!({"id": id}));
            } else {
                let _ = handle.emit("lan_peer_discovered", peer.clone());
            }
        }
    }
}

/// Handle a single incoming TCP session: identify the peer, receive messages, close.
async fn handle_incoming_session(
    stream: TcpStream,
    my_uuid: &[u8; 16],
    context: &IncomingSessionContext<'_>,
) -> Result<(), String> {
    // Extract sender IP before moving stream into Connection
    let sender_addr = stream
        .peer_addr()
        .map_err(|e| format!("Failed to read peer address: {}", e))?;
    let sender_ip = sender_addr.ip().to_string();

    let allowed_tailnet = match sender_addr.ip() {
        IpAddr::V4(ip) => context.tailnet_ips.lock().await.contains(&ip),
        _ => false,
    };
    if !is_current_lan_peer_ip(&sender_ip) && !allowed_tailnet {
        return Err(format!(
            "Rejected connection from {sender_ip}: outside the current LAN and connected tailnet"
        ));
    }

    let (conn, sender_id) = Connection::from_incoming(stream, my_uuid).await?;
    // Probes must never manufacture a visible sender: a UUID handshake does
    // not prove that the initiating device has a running listener.
    let first_message = match conn.recv_message().await? {
        None => return Ok(()),
        Some(super::protocol::Message::Discover) => {
            let alias = context.alias_rx.borrow().clone();
            conn.send_message(&super::protocol::Message::Identity {
                app: "LanDrop".into(),
                alias,
                device_type: context.device_type.to_string(),
            })
            .await?;
            return Ok(());
        }
        Some(super::protocol::Message::Identity { .. }) => {
            return Err("Unexpected identity response".into())
        }
        Some(super::protocol::Message::Done) => return Ok(()),
        Some(message) => message,
    };

    {
        let mut pending = context.pending_removals.lock().await;
        pending.remove(&sender_id);
    }

    // Look up output folder: per-peer override → global default → Downloads
    let out_folder = {
        let folders = context.receive_routing.peer_folders.lock().await;
        match folders.get(&sender_id) {
            Some(f) if !f.is_empty() => f.clone(),
            _ => {
                let default = context.receive_routing.default_out_folder.lock().await;
                if default.is_empty() {
                    String::new()
                } else {
                    default.clone()
                }
            }
        }
    };
    let sort_into_date_folder = *context.receive_routing.sort_by_date.lock().await;

    // Register sender in discovered_peers (ensures we can send back to them).
    // Always update the IP — mDNS might have stale data or never discovered them.
    let sender_alias = {
        let mut peers = context.discovered_peers.lock().await;
        if context.cancel.is_cancelled() {
            return Err("Discovery stopped before the incoming session started".into());
        }
        let peer = peers.entry(sender_id.clone()).or_insert_with(|| {
            DiscoveredPeer::new(
                sender_id.clone(),
                format!("Device-{}", &sender_id[..8]),
                "desktop".into(),
                sender_ip.clone(),
            )
        });
        peer.observe_route(&sender_ip);
        let alias = peer.alias.clone();
        let peer = peer.clone();
        drop(peers);
        // Always emit the peer to the frontend on inbound traffic.
        // This rehydrates the UI if discovery is stale or the user removed the chip locally.
        let _ = context.handle.emit("lan_peer_discovered", &peer);
        alias
    };

    // A clean close before the first control frame is a discovery probe. Once a
    // file transfer begins, the sender must finish it with Done. LanDrop 1.6.12
    // and older text senders close immediately after Text, so retain that one
    // legacy clean-EOF case during the protocol-v1 compatibility window.
    let mut received_control_message = false;
    let mut allow_legacy_text_eof = false;
    let mut first_message = Some(first_message);
    loop {
        let next = match first_message.take() {
            Some(message) => Some(message),
            None => conn.recv_message().await?,
        };
        let msg = match next {
            Some(msg) => msg,
            None if !received_control_message => return Ok(()),
            None if allow_legacy_text_eof => return Ok(()),
            None => return Err("Peer closed the session before sending Done".into()),
        };
        received_control_message = true;

        match msg {
            super::protocol::Message::Text { text } => {
                allow_legacy_text_eof = true;
                emit_android_receive_notification(
                    context.handle,
                    &sender_alias,
                    &notification_text_preview(&text),
                );
                let _ = context.handle.emit(
                    "lan_text_received",
                    serde_json::json!({"peer_id": sender_id, "text": text}),
                );
            }
            super::protocol::Message::File { name, size } => {
                allow_legacy_text_eof = false;
                // Legacy single-file sessions must participate in the same
                // start/terminal activity contract as batches. The frontend
                // uses it to avoid closing the app for an update mid-receive.
                let _ = context.handle.emit(
                    "lan_transfer_progress",
                    serde_json::json!({
                        "direction": "receive",
                        "phase": "start",
                        "total_bytes": size,
                        "total_files": 1,
                        "received_bytes": 0,
                        "received_files": 0,
                    }),
                );
                let path = match super::transfer::receive_file(
                    &conn,
                    &name,
                    size,
                    &out_folder,
                    sort_into_date_folder,
                    Some(context.handle),
                )
                .await
                {
                    Ok(path) => path,
                    Err(error) => {
                        emit_transfer_error(context.handle, "receive");
                        return Err(format!("File receive error: {error}"));
                    }
                };
                emit_android_receive_notification(
                    context.handle,
                    &sender_alias,
                    &format!("Received {}", name),
                );
                let _ = context.handle.emit(
                    "lan_files_received",
                    serde_json::json!({
                        "peer_id": sender_id,
                        "files": [&name],
                        "file_details": [{"name": &name, "path": &path, "size": size}]
                    }),
                );
                let _ = context.handle.emit(
                    "lan_transfer_progress",
                    serde_json::json!({
                        "direction": "receive",
                        "phase": "done",
                        "total_bytes": size,
                        "total_files": 1,
                        "received_bytes": size,
                        "received_files": 1,
                    }),
                );
            }
            super::protocol::Message::Batch { count } => {
                let files = match super::transfer::receive_batch(
                    &conn,
                    count,
                    &out_folder,
                    sort_into_date_folder,
                    Some(context.handle),
                )
                .await
                {
                    Ok(files) => files,
                    Err(error) => {
                        emit_transfer_error(context.handle, "receive");
                        return Err(format!("Batch receive error: {error}"));
                    }
                };
                let body = match files.len() {
                    0 => "Received files".to_string(),
                    1 => format!("Received {}", files[0].0),
                    n => format!("Received {} items", n),
                };
                emit_android_receive_notification(context.handle, &sender_alias, &body);
                let names: Vec<&str> = files.iter().map(|(n, _, _)| n.as_str()).collect();
                let details: Vec<serde_json::Value> = files
                    .iter()
                    .map(|(name, path, size)| {
                        serde_json::json!({"name": name, "path": path, "size": size})
                    })
                    .collect();
                let _ = context.handle.emit(
                    "lan_files_received",
                    serde_json::json!({
                        "peer_id": sender_id,
                        "files": names,
                        "file_details": details,
                    }),
                );
                return Ok(());
            }
            super::protocol::Message::Done => return Ok(()),
            super::protocol::Message::Dir { .. } => {
                return Err("Unexpected directory marker outside a batch".into());
            }
            super::protocol::Message::Discover | super::protocol::Message::Identity { .. } => {
                return Err("Unexpected discovery message during a transfer".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DiscoveredPeer;
    use super::{
        choose_peer_ipv4, is_bad_interface, is_same_lan_ipv4, is_usable_ipv4,
        local_interface_score, notification_text_preview, peer_address_score, private_ipv4_score,
        same_lan_probe_ips, service_fullname, service_instance_name,
    };
    use mdns_sd::ScopedIp;
    use std::net::{IpAddr, Ipv4Addr};

    fn scoped(a: u8, b: u8, c: u8, d: u8) -> ScopedIp {
        ScopedIp::from(IpAddr::V4(Ipv4Addr::new(a, b, c, d)))
    }

    #[test]
    fn same_device_merges_routes_and_prefers_lan_in_either_discovery_order() {
        for routes in [
            ["100.100.1.2", "192.168.1.2"],
            ["192.168.1.2", "100.100.1.2"],
        ] {
            let mut peer = DiscoveredPeer::new(
                "device-id".into(),
                "Colleague".into(),
                "desktop".into(),
                routes[0].into(),
            );
            peer.observe_route(routes[1]);
            assert_eq!(peer.route_ips(), ["192.168.1.2", "100.100.1.2"]);
            assert_eq!(peer.ip, "192.168.1.2");
            assert_eq!(peer.network, "lan");
            assert_eq!(peer.id, "device-id");
        }
    }

    #[test]
    fn losing_lan_preserves_tailnet_then_last_route_expires() {
        let mut peer = DiscoveredPeer::new(
            "device-id".into(),
            "Colleague".into(),
            "desktop".into(),
            "192.168.1.2".into(),
        );
        peer.observe_route("100.100.1.2");
        peer.forget_route_before("192.168.1.2", std::time::Instant::now());
        assert_eq!(peer.network, "tailscale");
        assert_eq!(peer.ip, "100.100.1.2");
        assert_eq!(peer.lan_ip, None);
        peer.forget_route_before("100.100.1.2", std::time::Instant::now());
        assert!(peer.ip.is_empty());
        assert!(peer.route_ips().is_empty());
    }

    #[test]
    fn stale_probe_result_does_not_remove_a_newly_rediscovered_route() {
        let mut peer = DiscoveredPeer::new(
            "device-id".into(),
            "Colleague".into(),
            "desktop".into(),
            "192.168.1.2".into(),
        );
        let old_probe = std::time::Instant::now();
        peer.observe_route("192.168.1.2");
        peer.forget_route_before("192.168.1.2", old_probe);
        assert_eq!(peer.ip, "192.168.1.2");
    }

    #[test]
    fn tailnet_address_is_not_a_lan_scan_interface() {
        assert!(!is_usable_ipv4(Ipv4Addr::new(100, 100, 1, 2)));
        assert!(is_bad_interface("Tailscale"));
        assert!(is_bad_interface("tailscale0"));
    }

    #[test]
    fn scores_private_ranges_in_preference_order() {
        // 192.168/16 beats 172.16-31 beats 10/8 beats anything public.
        assert!(
            private_ipv4_score(Ipv4Addr::new(192, 168, 1, 5))
                > private_ipv4_score(Ipv4Addr::new(172, 20, 1, 5))
        );
        assert!(
            private_ipv4_score(Ipv4Addr::new(172, 20, 1, 5))
                > private_ipv4_score(Ipv4Addr::new(10, 0, 0, 5))
        );
        assert!(
            private_ipv4_score(Ipv4Addr::new(10, 0, 0, 5))
                > private_ipv4_score(Ipv4Addr::new(8, 8, 8, 8))
        );
        // 172.15 and 172.32 are outside the private block.
        assert_eq!(private_ipv4_score(Ipv4Addr::new(172, 15, 0, 1)), 1);
        assert_eq!(private_ipv4_score(Ipv4Addr::new(172, 32, 0, 1)), 1);
        assert_eq!(private_ipv4_score(Ipv4Addr::new(172, 16, 0, 1)), 20);
        assert_eq!(private_ipv4_score(Ipv4Addr::new(172, 31, 0, 1)), 20);
    }

    #[test]
    fn rejects_unusable_ipv4_addresses() {
        assert!(is_usable_ipv4(Ipv4Addr::new(192, 168, 0, 2)));
        assert!(!is_usable_ipv4(Ipv4Addr::LOCALHOST));
        assert!(!is_usable_ipv4(Ipv4Addr::UNSPECIFIED));
        assert!(!is_usable_ipv4(Ipv4Addr::new(224, 0, 0, 251)));
        // Link-local autoconfiguration
        assert!(!is_usable_ipv4(Ipv4Addr::new(169, 254, 1, 1)));
    }

    #[test]
    fn penalizes_virtual_and_vpn_interfaces() {
        assert!(is_bad_interface("wg0"));
        assert!(is_bad_interface("vEthernet (WSL)"));
        assert!(is_bad_interface("Hyper-V Virtual Adapter"));
        assert!(!is_bad_interface("Wi-Fi"));
        assert!(!is_bad_interface("eth0"));

        let ip = Ipv4Addr::new(192, 168, 1, 10);
        assert!(local_interface_score("Wi-Fi", ip) > local_interface_score("wg0", ip));
        // Unusable addresses are ruled out entirely, whatever the interface.
        assert_eq!(local_interface_score("Wi-Fi", Ipv4Addr::LOCALHOST), -1000);
    }

    #[test]
    fn scores_peers_only_on_the_local_lan() {
        let local = Ipv4Addr::new(192, 168, 1, 10);
        assert!(is_same_lan_ipv4(local, Ipv4Addr::new(192, 168, 1, 44)));
        assert!(!is_same_lan_ipv4(local, Ipv4Addr::new(192, 168, 2, 44)));

        assert!(peer_address_score(local, Ipv4Addr::new(192, 168, 1, 44)) > 0);
        assert_eq!(
            peer_address_score(local, Ipv4Addr::new(10, 0, 0, 44)),
            -1000
        );
        assert_eq!(peer_address_score(local, Ipv4Addr::LOCALHOST), -1000);
    }

    #[test]
    fn chooses_the_same_lan_address_from_multiple_advertisements() {
        let local = Ipv4Addr::new(192, 168, 1, 10);
        let advertised = [
            scoped(10, 8, 0, 6),     // VPN
            scoped(192, 168, 1, 44), // real LAN
            scoped(172, 17, 0, 2),   // docker bridge
        ];
        assert_eq!(
            choose_peer_ipv4(advertised.iter(), local),
            Some(Ipv4Addr::new(192, 168, 1, 44))
        );

        // Nothing on this LAN: no candidate at all.
        let off_lan = [scoped(10, 8, 0, 6)];
        assert_eq!(choose_peer_ipv4(off_lan.iter(), local), None);
    }

    #[test]
    fn probe_range_covers_the_lan_without_self() {
        let local = Ipv4Addr::new(192, 168, 1, 10);
        let probes = same_lan_probe_ips(local);
        assert_eq!(probes.len(), 253);
        assert!(!probes.contains(&local));
        assert!(probes.contains(&Ipv4Addr::new(192, 168, 1, 1)));
        assert!(probes.contains(&Ipv4Addr::new(192, 168, 1, 254)));
        assert!(!probes.contains(&Ipv4Addr::new(192, 168, 1, 0)));
        assert!(!probes.contains(&Ipv4Addr::new(192, 168, 1, 255)));
    }

    #[test]
    fn builds_notification_previews_within_bounds() {
        assert_eq!(notification_text_preview("   "), "New message received");
        assert_eq!(notification_text_preview("  hello  "), "hello");

        let long = "a".repeat(300);
        let preview = notification_text_preview(&long);
        assert!(preview.ends_with("..."));
        assert_eq!(preview.chars().count(), 163);

        // Multi-byte input is truncated by chars, never mid-encoding.
        let emoji = "🦀".repeat(300);
        let preview = notification_text_preview(&emoji);
        assert!(preview.ends_with("..."));
        assert_eq!(preview.chars().count(), 163);
    }

    #[test]
    fn derives_stable_service_names_from_the_device_id() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert_eq!(service_instance_name(id), "LanDrop-550e8400");
        assert_eq!(
            service_fullname(id),
            "LanDrop-550e8400._landrop._tcp.local."
        );
    }
}
