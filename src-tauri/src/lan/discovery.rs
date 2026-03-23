use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::time;
use tauri::{AppHandle, Emitter};

use super::protocol::{BEACON_MAGIC, BEACON_PORT, TCP_PORT};
use super::transfer::Connection;

pub async fn run_discovery(
    handle: AppHandle,
    running: Arc<Mutex<bool>>,
    connection: Arc<Mutex<Option<Arc<Connection>>>>,
    code_hash: Arc<Mutex<Vec<u8>>>,
    out_folder: Arc<Mutex<String>>,
) {
    // Bind UDP socket for beacon
    let udp = match UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, BEACON_PORT)).await {
        Ok(s) => {
            let _ = s.set_broadcast(true);
            Arc::new(s)
        }
        Err(e) => {
            eprintln!("Failed to bind UDP beacon: {}", e);
            return;
        }
    };

    // Bind TCP listener
    let tcp_listener = match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, TCP_PORT)).await {
        Ok(l) => Arc::new(l),
        Err(e) => {
            eprintln!("Failed to bind TCP: {}", e);
            return;
        }
    };

    let local_ips = get_local_ips();

    // Compute subnet-directed broadcast addresses for more reliable discovery
    let subnet_broadcasts = get_subnet_broadcasts();

    // Spawn beacon sender
    let udp_send = udp.clone();
    let running_send = running.clone();
    let code_hash_send = code_hash.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            if !*running_send.lock().await {
                break;
            }
            let hash = code_hash_send.lock().await.clone();
            if hash.is_empty() {
                continue;
            }
            let mut beacon = Vec::with_capacity(24);
            beacon.extend_from_slice(BEACON_MAGIC);
            beacon.extend_from_slice(&hash);

            // Send to limited broadcast
            let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, BEACON_PORT);
            let _ = udp_send.send_to(&beacon, broadcast_addr).await;

            // Also send to each subnet-directed broadcast (e.g. 192.168.1.255)
            for addr in &subnet_broadcasts {
                let _ = udp_send.send_to(&beacon, SocketAddrV4::new(*addr, BEACON_PORT)).await;
            }
        }
    });

    // Spawn beacon listener + TCP acceptor in parallel
    let running_recv = running.clone();
    let connection_recv = connection.clone();
    let code_hash_recv = code_hash.clone();
    let handle_recv = handle.clone();
    let out_folder_recv = out_folder.clone();
    let local_ips_recv = local_ips.clone();

    // TCP listener task
    let running_tcp = running.clone();
    let connection_tcp = connection.clone();
    let code_hash_tcp = code_hash.clone();
    let handle_tcp = handle.clone();
    let out_folder_tcp = out_folder.clone();
    let tcp_task = tokio::spawn(async move {
        loop {
            if !*running_tcp.lock().await {
                break;
            }
            let accept = time::timeout(Duration::from_secs(1), tcp_listener.accept()).await;
            if let Ok(Ok((stream, addr))) = accept {
                if connection_tcp.lock().await.is_some() {
                    continue; // Already connected
                }
                let hash = code_hash_tcp.lock().await.clone();
                if let Ok(conn) = Connection::from_incoming(stream, &hash).await {
                    let peer_ip = addr.ip().to_string();
                    *connection_tcp.lock().await = Some(conn.clone());
                    let _ = handle_tcp.emit("lan_connected", serde_json::json!({"peer_ip": peer_ip}));

                    // Start receive loop
                    start_recv_loop(
                        handle_tcp.clone(),
                        conn,
                        connection_tcp.clone(),
                        running_tcp.clone(),
                        out_folder_tcp.clone(),
                    )
                    .await;
                }
            }
        }
    });

    // UDP listener + outbound connector task
    let udp_listen = udp.clone();
    let udp_task = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        loop {
            if !*running_recv.lock().await {
                break;
            }
            let recv = time::timeout(Duration::from_secs(1), udp_listen.recv_from(&mut buf)).await;
            if let Ok(Ok((len, addr))) = recv {
                if len < 24 {
                    continue;
                }
                // Ignore our own beacons
                if local_ips_recv.contains(&addr.ip().to_string()) {
                    continue;
                }
                if &buf[..8] != BEACON_MAGIC {
                    continue;
                }
                let peer_hash = &buf[8..24];
                let our_hash = code_hash_recv.lock().await;
                if peer_hash != our_hash.as_slice() {
                    continue;
                }
                drop(our_hash);

                // Same code hash — try to connect if not already connected
                if connection_recv.lock().await.is_some() {
                    continue;
                }

                let peer_ip = addr.ip();
                let tcp_addr = SocketAddr::new(peer_ip, TCP_PORT);
                if let Ok(stream) = time::timeout(
                    Duration::from_secs(3),
                    TcpStream::connect(tcp_addr),
                )
                .await
                {
                    if let Ok(stream) = stream {
                        let hash = code_hash_recv.lock().await.clone();
                        if let Ok(conn) = Connection::from_outgoing(stream, &hash).await {
                            let peer_ip_str = peer_ip.to_string();
                            *connection_recv.lock().await = Some(conn.clone());
                            let _ = handle_recv.emit(
                                "lan_connected",
                                serde_json::json!({"peer_ip": peer_ip_str}),
                            );

                            start_recv_loop(
                                handle_recv.clone(),
                                conn,
                                connection_recv.clone(),
                                running_recv.clone(),
                                out_folder_recv.clone(),
                            )
                            .await;
                        }
                    }
                }
            }
        }
    });

    let _ = tokio::join!(tcp_task, udp_task);
}

async fn start_recv_loop(
    handle: AppHandle,
    conn: Arc<Connection>,
    connection: Arc<Mutex<Option<Arc<Connection>>>>,
    running: Arc<Mutex<bool>>,
    out_folder: Arc<Mutex<String>>,
) {
    // Spawn keep-alive ping sender
    let ping_conn = conn.clone();
    let ping_running = running.clone();
    let ping_connection = connection.clone();
    let ping_handle = handle.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;
            if !*ping_running.lock().await { break; }
            if ping_conn.send_message(&super::protocol::Message::Ping).await.is_err() {
                // Connection broke — clean up properly
                *ping_connection.lock().await = None;
                let _ = ping_handle.emit("lan_disconnected", serde_json::json!({}));
                break;
            }
        }
    });

    let recv_conn = conn.clone();
    tokio::spawn(async move {
        loop {
            if !*running.lock().await {
                break;
            }

            // Use the Arc<Connection> directly — no outer lock needed for recv!
            let msg = match time::timeout(Duration::from_secs(30), recv_conn.recv_message()).await {
                Ok(Ok(msg)) => msg,
                Ok(Err(_)) | Err(_) => {
                    *connection.lock().await = None;
                    let _ = handle.emit("lan_disconnected", serde_json::json!({}));
                    break;
                }
            };

            match msg {
                super::protocol::Message::Text { text } => {
                    let _ = handle.emit(
                        "lan_text_received",
                        serde_json::json!({"text": text}),
                    );
                }
                super::protocol::Message::Ping => {
                    let _ = recv_conn.send_message(&super::protocol::Message::Pong).await;
                }
                super::protocol::Message::File { name, size } => {
                    let folder = out_folder.lock().await.clone();
                    match super::transfer::receive_file(&recv_conn, &name, size, &folder, Some(&handle)).await {
                        Ok(path) => {
                            let _ = handle.emit(
                                "lan_files_received",
                                serde_json::json!({
                                    "files": [&name],
                                    "file_details": [{
                                        "name": &name,
                                        "path": &path,
                                        "size": size,
                                    }]
                                }),
                            );
                        }
                        Err(e) => eprintln!("File receive error: {}", e),
                    }
                }
                super::protocol::Message::Batch { count } => {
                    let folder = out_folder.lock().await.clone();
                    match super::transfer::receive_batch(&recv_conn, count, &folder, Some(&handle)).await {
                        Ok(files) => {
                            let names: Vec<&str> = files.iter().map(|(n, _, _)| n.as_str()).collect();
                            let details: Vec<serde_json::Value> = files
                                .iter()
                                .map(|(name, path, size)| {
                                    serde_json::json!({
                                        "name": name,
                                        "path": path,
                                        "size": size,
                                    })
                                })
                                .collect();
                            let _ = handle.emit(
                                "lan_files_received",
                                serde_json::json!({
                                    "files": names,
                                    "file_details": details,
                                }),
                            );
                        }
                        Err(e) => eprintln!("Batch receive error: {}", e),
                    }
                }
                _ => {}
            }
        }
    });
}

fn get_local_ips() -> Vec<String> {
    local_ip_address::list_afinet_netifas()
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, ip)| ip.is_ipv4())
        .map(|(_, ip)| ip.to_string())
        .collect()
}

/// Compute subnet-directed broadcast addresses (e.g. 192.168.1.255 for /24)
/// This is more reliable than 255.255.255.255 on many routers/firewalls
fn get_subnet_broadcasts() -> Vec<Ipv4Addr> {
    let mut addrs = Vec::new();
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();
    for (_, ip) in &interfaces {
        if let std::net::IpAddr::V4(v4) = ip {
            if v4.is_loopback() { continue; }
            let octets = v4.octets();
            // Assume /24 subnet (most common home/office network)
            // Broadcast = x.x.x.255
            let broadcast = Ipv4Addr::new(octets[0], octets[1], octets[2], 255);
            if !addrs.contains(&broadcast) {
                addrs.push(broadcast);
            }
        }
    }
    addrs
}
