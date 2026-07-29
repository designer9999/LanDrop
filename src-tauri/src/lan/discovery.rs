use mdns_sd::{ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{AppHandle, Emitter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tokio::time;

use super::identity::{normalize_uuid, DeviceIdentity};
use super::protocol::{MDNS_SERVICE_TYPE, TCP_PORT};
use super::transfer::{probe_peer_id, Connection};

const MAX_INCOMING_SESSIONS: usize = 32;

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
        .unwrap_or(false);
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
    !(ip.is_loopback()
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
}

/// Run mDNS-based discovery: register this device, browse for others, accept TCP transfers.
pub async fn run_discovery(
    handle: AppHandle,
    running: Arc<AtomicBool>,
    identity: DeviceIdentity,
    discovered_peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    receive_routing: ReceiveRoutingState,
    alias: Arc<Mutex<String>>,
) {
    // Get our local LAN IP
    let local_ip = match get_local_ipv4() {
        Some(ip) => ip,
        None => {
            emit_log(
                &handle,
                "error",
                "No LAN IPv4 address found — cannot start discovery",
            );
            running.store(false, Ordering::SeqCst);
            return;
        }
    };
    emit_log(&handle, "info", &format!("Local IP: {}", local_ip));

    // Create mDNS daemon
    let mdns = match ServiceDaemon::new() {
        Ok(d) => {
            emit_log(&handle, "success", "mDNS daemon started");
            d
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
            running.store(false, Ordering::SeqCst);
            return;
        }
    };

    // Register our service
    let my_id = normalize_uuid(&identity.id).unwrap_or_else(|| identity.id.clone());
    let current_alias = alias.lock().await.clone();

    let properties = [
        ("id", my_id.as_str()),
        ("alias", current_alias.as_str()),
        ("dtype", identity.device_type.as_str()),
    ];

    let host_name = format!("landrop-{}.local.", &my_id[..8]);
    let instance_name = format!("LanDrop-{}", &my_id[..8]);

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
                &handle,
                "success",
                &format!(
                    "Registered as \"{}\" on {}:{}",
                    current_alias, local_ip, TCP_PORT
                ),
            ),
            Err(e) => emit_log(
                &handle,
                "error",
                &format!("Failed to register mDNS service: {}", e),
            ),
        },
        Err(e) => {
            emit_log(
                &handle,
                "error",
                &format!("Failed to create mDNS service info: {}", e),
            );
        }
    }

    // Browse for other instances
    let browse_receiver = match mdns.browse(MDNS_SERVICE_TYPE) {
        Ok(r) => {
            emit_log(
                &handle,
                "success",
                "Browsing for LanDrop devices on network...",
            );
            r
        }
        Err(e) => {
            emit_log(&handle, "error", &format!("Failed to browse mDNS: {}", e));
            running.store(false, Ordering::SeqCst);
            return;
        }
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
                    running.store(false, Ordering::SeqCst);
                    return;
                }
            }
        }
        let Some(listener) = listener_opt else {
            running.store(false, Ordering::SeqCst);
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

    let running_mdns = running.clone();
    let handle_mdns = handle.clone();
    let peers_mdns = discovered_peers.clone();
    let my_id_mdns = my_id.clone();
    let pending_mdns = pending_removals.clone();
    let local_ip_mdns = local_ip;
    let mdns_processor = tokio::spawn(async move {
        let grace_period = Duration::from_secs(15);
        let mut last_sweep = Instant::now();

        loop {
            if !running_mdns.load(Ordering::Relaxed) {
                break;
            }

            // ── Sweep pending removals every 5 seconds ──
            if last_sweep.elapsed() >= Duration::from_secs(5) {
                last_sweep = Instant::now();
                let mut pending = pending_mdns.lock().await;
                let expired: Vec<(String, String, u16)> = pending
                    .iter()
                    .filter(|(_, (at, _, _))| at.elapsed() >= grace_period)
                    .map(|(id, (_, ip, port))| (id.clone(), ip.clone(), *port))
                    .collect();
                for (id, ip, port) in expired {
                    pending.remove(&id);
                    // TCP liveness check — try to connect before marking offline
                    let addr = format!("{}:{}", ip, port);
                    let alive = matches!(
                        tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&addr))
                            .await,
                        Ok(Ok(_))
                    );
                    if alive {
                        // Peer is still alive — mDNS lied. Re-add to discovered.
                        emit_log(
                            &handle_mdns,
                            "info",
                            &format!("Peer {} still alive (mDNS removal was false)", &id[..8]),
                        );
                    } else {
                        // Peer is genuinely gone
                        let mut peers = peers_mdns.lock().await;
                        peers.remove(&id);
                        drop(peers);
                        let _ = handle_mdns.emit("lan_peer_lost", serde_json::json!({"id": id}));
                        emit_log(
                            &handle_mdns,
                            "warn",
                            &format!("Peer {} confirmed offline after TCP check", &id[..8]),
                        );
                    }
                }
            }

            // Poll mDNS events with timeout so we can check `running`
            if let Ok(Ok(Ok(event))) = tokio::time::timeout(
                Duration::from_secs(1),
                tokio::task::spawn_blocking({
                    let recv = browse_receiver.clone();
                    move || recv.recv_timeout(Duration::from_secs(1))
                }),
            )
            .await
            {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
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

                        let peer = DiscoveredPeer {
                            id: peer_id.clone(),
                            alias: peer_alias,
                            device_type: peer_dtype,
                            ip,
                            port: info.get_port(),
                        };

                        let mut peers = peers_mdns.lock().await;
                        peers.insert(peer_id.clone(), peer.clone());
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
    });

    // ── Task 2: TCP listener — accepts incoming transfers ──
    let running_tcp = running.clone();
    let handle_tcp = handle.clone();
    let my_uuid = identity.id_bytes();
    let peers_tcp = discovered_peers.clone();
    let pending_tcp = pending_removals.clone();
    let incoming_session_slots = Arc::new(Semaphore::new(MAX_INCOMING_SESSIONS));
    let tcp_acceptor = tokio::spawn(async move {
        let mut listener: Arc<TcpListener> = tcp_listener;
        let mut consecutive_errors: u32 = 0;
        let mut last_capacity_warning: Option<Instant> = None;

        loop {
            if !running_tcp.load(Ordering::Relaxed) {
                break;
            }

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
                        time::sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                }
            }

            let accept = time::timeout(Duration::from_secs(1), listener.accept()).await;
            match accept {
                Ok(Ok((stream, _addr))) => {
                    consecutive_errors = 0;
                    let permit = match incoming_session_slots.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            if last_capacity_warning
                                .map(|at| at.elapsed() >= Duration::from_secs(5))
                                .unwrap_or(true)
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

                    tokio::spawn(async move {
                        let _permit = permit;
                        let context = IncomingSessionContext {
                            handle: &handle_session,
                            receive_routing: &receive_routing,
                            discovered_peers: &peers_ref,
                            pending_removals: &pending_ref,
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
                Ok(Err(_)) => {
                    consecutive_errors += 1;
                }
                Err(_) => {} // Timeout — normal, loop continues
            }
        }
    });

    // ── Task 3: same-LAN TCP healer scan ──
    // mDNS can be lost or polluted by VPN/virtual interfaces. Probe only this
    // machine's /24 LAN and recover peers by their UUID handshake.
    let running_scan = running.clone();
    let handle_scan = handle.clone();
    let peers_scan = discovered_peers.clone();
    let my_id_scan = my_id.clone();
    let my_uuid_scan = identity.id_bytes();
    let lan_scanner = tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;
            if !running_scan.load(Ordering::Relaxed) {
                break;
            }

            let Some(local_ip) = get_local_ipv4() else {
                continue;
            };

            let mut probes = JoinSet::new();
            for ip in same_lan_probe_ips(local_ip) {
                let ip_string = ip.to_string();
                let uuid = my_uuid_scan;
                probes.spawn(async move {
                    probe_peer_id(&ip_string, &uuid)
                        .await
                        .ok()
                        .map(|peer_id| (peer_id, ip_string))
                });
            }

            while let Some(result) = probes.join_next().await {
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
                    .map(|peer| peer.ip != ip)
                    .unwrap_or(true);
                let peer = DiscoveredPeer {
                    id: peer_id.clone(),
                    alias: peers
                        .get(&peer_id)
                        .map(|p| p.alias.clone())
                        .unwrap_or_else(|| format!("Device-{}", &peer_id[..8])),
                    device_type: peers
                        .get(&peer_id)
                        .map(|p| p.device_type.clone())
                        .unwrap_or_else(|| "desktop".to_string()),
                    ip: ip.clone(),
                    port: TCP_PORT,
                };
                peers.insert(peer_id.clone(), peer.clone());
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

    let _ = tokio::join!(mdns_processor, tcp_acceptor, lan_scanner);

    // Graceful shutdown — send mDNS goodbye
    let _ = mdns.shutdown();

    // Always reset running flag on exit so start() can succeed next time
    running.store(false, Ordering::SeqCst);
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

    match (get_local_ipv4(), sender_addr.ip()) {
        (Some(local_ip), IpAddr::V4(sender_v4)) if is_same_lan_ipv4(local_ip, sender_v4) => {}
        (Some(local_ip), IpAddr::V4(sender_v4)) => {
            return Err(format!(
                "Rejected non-LAN incoming connection from {} (local LAN is {})",
                sender_v4, local_ip
            ));
        }
        (Some(local_ip), other) => {
            return Err(format!(
                "Rejected non-IPv4 incoming connection from {} (local LAN is {})",
                other, local_ip
            ));
        }
        (None, _) => return Err("Rejected incoming connection: no local LAN IPv4".into()),
    }

    let (conn, sender_id) = Connection::from_incoming(stream, my_uuid).await?;

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
        let peer = DiscoveredPeer {
            id: sender_id.clone(),
            alias: peers
                .get(&sender_id)
                .map(|p| p.alias.clone())
                .unwrap_or_else(|| format!("Device-{}", &sender_id[..8])),
            device_type: peers
                .get(&sender_id)
                .map(|p| p.device_type.clone())
                .unwrap_or_else(|| "desktop".to_string()),
            ip: sender_ip.clone(),
            port: TCP_PORT,
        };
        let alias = peer.alias.clone();
        peers.insert(sender_id.clone(), peer.clone());
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
    loop {
        let msg = match conn.recv_message().await? {
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
        }
    }
}
