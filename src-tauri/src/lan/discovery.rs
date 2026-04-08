use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time;

use super::identity::{normalize_uuid, DeviceIdentity};
use super::protocol::{MDNS_SERVICE_TYPE, TCP_PORT};
use super::transfer::Connection;

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

/// Get the local LAN IPv4 address
pub fn get_local_ipv4() -> Option<Ipv4Addr> {
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();
    let vpn_keywords = [
        "tun",
        "tap",
        "wg",
        "vpn",
        "proton",
        "nord",
        "mullvad",
        "wireguard",
    ];

    for (name, ip) in &interfaces {
        if let IpAddr::V4(v4) = ip {
            if v4.is_loopback() {
                continue;
            }
            let name_lower = name.to_lowercase();
            if vpn_keywords.iter().any(|k| name_lower.contains(k)) {
                continue;
            }
            let octets = v4.octets();
            let is_lan = (octets[0] == 192 && octets[1] == 168)
                || octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]));
            if is_lan {
                return Some(*v4);
            }
        }
    }
    // Fallback: any non-loopback IPv4
    for (_, ip) in &interfaces {
        if let IpAddr::V4(v4) = ip {
            if !v4.is_loopback() {
                return Some(*v4);
            }
        }
    }
    None
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
            emit_log(
                &handle,
                "error",
                &format!("Failed to create mDNS daemon: {}", e),
            );
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
            return;
        }
    };

    // Bind TCP listener
    let tcp_listener =
        match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, TCP_PORT)).await {
            Ok(l) => {
                emit_log(
                    &handle,
                    "success",
                    &format!("TCP listener on port {}", TCP_PORT),
                );
                Arc::new(l)
            }
            Err(e) => {
                emit_log(
                    &handle,
                    "error",
                    &format!("Failed to bind TCP port {}: {}", TCP_PORT, e),
                );
                return;
            }
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
                    let alive =
                        tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(&addr))
                            .await;
                    if alive.is_ok() && alive.unwrap().is_ok() {
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

                        // Get the first IPv4 address
                        let ip = info
                            .get_addresses()
                            .iter()
                            .find(|a| a.is_ipv4())
                            .map(|a| a.to_string())
                            .unwrap_or_default();

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
    let tcp_acceptor = tokio::spawn(async move {
        let mut listener: Arc<TcpListener> = tcp_listener;
        let mut consecutive_errors: u32 = 0;

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
                    let handle_session = handle_tcp.clone();
                    let receive_routing = receive_routing.clone();
                    let peers_ref = peers_tcp.clone();
                    let pending_ref = pending_tcp.clone();

                    tokio::spawn(async move {
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

    let _ = tokio::join!(mdns_processor, tcp_acceptor);

    // Graceful shutdown — send mDNS goodbye
    let _ = mdns.shutdown();
}

/// Handle a single incoming TCP session: authenticate, receive messages, close.
async fn handle_incoming_session(
    stream: TcpStream,
    my_uuid: &[u8; 16],
    context: &IncomingSessionContext<'_>,
) -> Result<(), String> {
    // Extract sender IP before moving stream into Connection
    let sender_ip = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();

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
    {
        let mut peers = context.discovered_peers.lock().await;
        let is_new = !peers.contains_key(&sender_id);
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
        peers.insert(sender_id.clone(), peer.clone());
        drop(peers);
        if is_new {
            let _ = context.handle.emit("lan_peer_discovered", &peer);
        }
    }

    // Read messages until connection closes
    loop {
        let msg = match conn.recv_message().await {
            Ok(msg) => msg,
            Err(_) => break,
        };

        match msg {
            super::protocol::Message::Text { text } => {
                let _ = context.handle.emit(
                    "lan_text_received",
                    serde_json::json!({"peer_id": sender_id, "text": text}),
                );
            }
            super::protocol::Message::File { name, size } => {
                match super::transfer::receive_file(
                    &conn,
                    &name,
                    size,
                    &out_folder,
                    sort_into_date_folder,
                    Some(context.handle),
                )
                .await
                {
                    Ok(path) => {
                        let _ = context.handle.emit(
                            "lan_files_received",
                            serde_json::json!({
                                "peer_id": sender_id,
                                "files": [&name],
                                "file_details": [{"name": &name, "path": &path, "size": size}]
                            }),
                        );
                    }
                    Err(e) => emit_log(
                        context.handle,
                        "error",
                        &format!("File receive error: {}", e),
                    ),
                }
            }
            super::protocol::Message::Batch { count } => {
                match super::transfer::receive_batch(
                    &conn,
                    count,
                    &out_folder,
                    sort_into_date_folder,
                    Some(context.handle),
                )
                .await
                {
                    Ok(files) => {
                        let names: Vec<&str> = files.iter().map(|(n, _, _)| n.as_str()).collect();
                        let details: Vec<serde_json::Value> = files.iter()
                            .map(|(name, path, size)| serde_json::json!({"name": name, "path": path, "size": size}))
                            .collect();
                        let _ = context.handle.emit(
                            "lan_files_received",
                            serde_json::json!({
                                "peer_id": sender_id,
                                "files": names,
                                "file_details": details,
                            }),
                        );
                    }
                    Err(e) => emit_log(
                        context.handle,
                        "error",
                        &format!("Batch receive error: {}", e),
                    ),
                }
            }
            super::protocol::Message::Done => break,
            _ => {}
        }
    }

    Ok(())
}
