use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time;
use tauri::{AppHandle, Emitter};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;

use super::identity::DeviceIdentity;
use super::protocol::{TCP_PORT, MDNS_SERVICE_TYPE};
use super::transfer::Connection;

/// Emit a log event to the frontend debug panel
fn emit_log(handle: &AppHandle, level: &str, text: &str) {
    let _ = handle.emit("lan_log", serde_json::json!({
        "level": level,
        "text": text,
    }));
    eprintln!("[LAN {}] {}", level, text);
}

/// Get the local LAN IPv4 address
fn get_local_ipv4() -> Option<Ipv4Addr> {
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();
    let vpn_keywords = ["tun", "tap", "wg", "vpn", "proton", "nord", "mullvad", "wireguard"];

    for (name, ip) in &interfaces {
        if let IpAddr::V4(v4) = ip {
            if v4.is_loopback() { continue; }
            let name_lower = name.to_lowercase();
            if vpn_keywords.iter().any(|k| name_lower.contains(k)) { continue; }
            let octets = v4.octets();
            let is_lan = (octets[0] == 192 && octets[1] == 168)
                || octets[0] == 10
                || (octets[0] == 172 && (16..=31).contains(&octets[1]));
            if is_lan { return Some(*v4); }
        }
    }
    // Fallback: any non-loopback IPv4
    for (_, ip) in &interfaces {
        if let IpAddr::V4(v4) = ip {
            if !v4.is_loopback() { return Some(*v4); }
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

/// Run mDNS-based discovery: register this device, browse for others, accept TCP transfers.
pub async fn run_discovery(
    handle: AppHandle,
    running: Arc<AtomicBool>,
    identity: DeviceIdentity,
    discovered_peers: Arc<Mutex<HashMap<String, DiscoveredPeer>>>,
    peer_folders: Arc<Mutex<HashMap<String, String>>>,
    default_out_folder: Arc<Mutex<String>>,
    alias: Arc<Mutex<String>>,
) {
    // Get our local LAN IP
    let local_ip = match get_local_ipv4() {
        Some(ip) => ip,
        None => {
            emit_log(&handle, "error", "No LAN IPv4 address found — cannot start discovery");
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
            emit_log(&handle, "error", &format!("Failed to create mDNS daemon: {}", e));
            return;
        }
    };

    // Register our service
    let my_id = identity.id.clone();
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
        &local_ip.to_string(),
        TCP_PORT,
        &properties[..],
    ) {
        Ok(service) => {
            match mdns.register(service) {
                Ok(_) => emit_log(&handle, "success", &format!(
                    "Registered as \"{}\" on {}:{}", current_alias, local_ip, TCP_PORT
                )),
                Err(e) => emit_log(&handle, "error", &format!("Failed to register mDNS service: {}", e)),
            }
        }
        Err(e) => {
            emit_log(&handle, "error", &format!("Failed to create mDNS service info: {}", e));
        }
    }

    // Browse for other instances
    let browse_receiver = match mdns.browse(MDNS_SERVICE_TYPE) {
        Ok(r) => {
            emit_log(&handle, "success", "Browsing for LanDrop devices on network...");
            r
        }
        Err(e) => {
            emit_log(&handle, "error", &format!("Failed to browse mDNS: {}", e));
            return;
        }
    };

    // Bind TCP listener
    let tcp_listener = match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, TCP_PORT)).await {
        Ok(l) => {
            emit_log(&handle, "success", &format!("TCP listener on port {}", TCP_PORT));
            Arc::new(l)
        }
        Err(e) => {
            emit_log(&handle, "error", &format!("Failed to bind TCP port {}: {}", TCP_PORT, e));
            return;
        }
    };

    // ── Task 1: mDNS event processor ──
    let running_mdns = running.clone();
    let handle_mdns = handle.clone();
    let peers_mdns = discovered_peers.clone();
    let my_id_mdns = my_id.clone();
    let mdns_processor = tokio::spawn(async move {
        loop {
            if !running_mdns.load(Ordering::Relaxed) {
                break;
            }
            // Poll mDNS events with timeout so we can check `running`
            match tokio::time::timeout(Duration::from_secs(1), tokio::task::spawn_blocking({
                let recv = browse_receiver.clone();
                move || recv.recv_timeout(Duration::from_secs(1))
            })).await {
                Ok(Ok(Ok(event))) => {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            // Extract peer info from TXT records
                            let props = info.get_properties();
                            let peer_id = props.get_property_val_str("id")
                                .unwrap_or_default().to_string();
                            let peer_alias = props.get_property_val_str("alias")
                                .unwrap_or_default().to_string();
                            let peer_dtype = props.get_property_val_str("dtype")
                                .unwrap_or("desktop").to_string();

                            // Skip our own service
                            if peer_id == my_id_mdns || peer_id.is_empty() {
                                continue;
                            }

                            // Get the first IPv4 address
                            let ip = info.get_addresses()
                                .iter()
                                .find(|a| a.is_ipv4())
                                .map(|a| a.to_string())
                                .unwrap_or_default();

                            if ip.is_empty() {
                                continue;
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
                            // Try to find and remove the peer
                            let mut peers = peers_mdns.lock().await;
                            let removed_id = peers.iter()
                                .find(|(_, p)| fullname.contains(&p.id[..8]))
                                .map(|(id, _)| id.clone());
                            if let Some(id) = removed_id {
                                peers.remove(&id);
                                drop(peers);
                                let _ = handle_mdns.emit(
                                    "lan_peer_lost",
                                    serde_json::json!({"id": id}),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                _ => {} // Timeout or channel error — loop continues
            }
        }
    });

    // ── Task 2: TCP listener — accepts incoming transfers ──
    let running_tcp = running.clone();
    let handle_tcp = handle.clone();
    let my_uuid = identity.id_bytes();
    let peers_tcp = discovered_peers.clone();
    let tcp_acceptor = tokio::spawn(async move {
        loop {
            if !running_tcp.load(Ordering::Relaxed) {
                break;
            }
            let accept = time::timeout(Duration::from_secs(1), tcp_listener.accept()).await;
            if let Ok(Ok((stream, _addr))) = accept {
                let handle_session = handle_tcp.clone();
                let folder_map = peer_folders.clone();
                let default_folder = default_out_folder.clone();
                let peers_ref = peers_tcp.clone();

                tokio::spawn(async move {
                    match handle_incoming_session(stream, &my_uuid, &handle_session, &folder_map, &default_folder, &peers_ref).await {
                        Ok(_) => {}
                        Err(e) => {
                            let _ = handle_session.emit("lan_log", serde_json::json!({
                                "level": "error",
                                "text": format!("Incoming session error: {}", e),
                            }));
                            eprintln!("Incoming session error: {}", e);
                        }
                    }
                });
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
    handle: &AppHandle,
    peer_folders: &Mutex<HashMap<String, String>>,
    default_folder: &Mutex<String>,
    discovered_peers: &Mutex<HashMap<String, DiscoveredPeer>>,
) -> Result<(), String> {
    let (conn, sender_id) = Connection::from_incoming(stream, my_uuid).await?;

    // Look up output folder: per-peer override → global default → Downloads
    let out_folder = {
        let folders = peer_folders.lock().await;
        match folders.get(&sender_id) {
            Some(f) if !f.is_empty() => f.clone(),
            _ => {
                let default = default_folder.lock().await;
                if default.is_empty() { String::new() } else { default.clone() }
            }
        }
    };

    // If we don't know this sender yet (direct TCP without mDNS), note them
    {
        let peers = discovered_peers.lock().await;
        if !peers.contains_key(&sender_id) {
            drop(peers);
            // Emit as discovered so UI knows about them
            let _ = handle.emit("lan_peer_discovered", serde_json::json!({
                "id": sender_id,
                "alias": format!("Device-{}", &sender_id[..8]),
                "device_type": "unknown",
                "ip": "",
                "port": TCP_PORT,
            }));
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
                let _ = handle.emit(
                    "lan_text_received",
                    serde_json::json!({"peer_id": sender_id, "text": text}),
                );
            }
            super::protocol::Message::File { name, size } => {
                match super::transfer::receive_file(&conn, &name, size, &out_folder, Some(handle)).await {
                    Ok(path) => {
                        let _ = handle.emit(
                            "lan_files_received",
                            serde_json::json!({
                                "peer_id": sender_id,
                                "files": [&name],
                                "file_details": [{"name": &name, "path": &path, "size": size}]
                            }),
                        );
                    }
                    Err(e) => emit_log(handle, "error", &format!("File receive error: {}", e)),
                }
            }
            super::protocol::Message::Batch { count } => {
                match super::transfer::receive_batch(&conn, count, &out_folder, Some(handle)).await {
                    Ok(files) => {
                        let names: Vec<&str> = files.iter().map(|(n, _, _)| n.as_str()).collect();
                        let details: Vec<serde_json::Value> = files.iter()
                            .map(|(name, path, size)| serde_json::json!({"name": name, "path": path, "size": size}))
                            .collect();
                        let _ = handle.emit(
                            "lan_files_received",
                            serde_json::json!({
                                "peer_id": sender_id,
                                "files": names,
                                "file_details": details,
                            }),
                        );
                    }
                    Err(e) => emit_log(handle, "error", &format!("Batch receive error: {}", e)),
                }
            }
            super::protocol::Message::Done => break,
            _ => {}
        }
    }

    Ok(())
}
