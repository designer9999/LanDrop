# LAN File Sharing — On-Demand TCP Architecture

## Why On-Demand (Not Persistent TCP)

Persistent TCP with ping/pong keepalive causes rapid disconnect cycling on real networks. Wi-Fi packets get dropped, one side detects "timeout" before the other, ghost connections pile up, and both sides fight over who's connected. This is how it breaks:

1. Ping timeout fires on side A → emits disconnect
2. Side B still thinks it's connected → sends into dead socket
3. Both sides try to reconnect simultaneously → race condition
4. Repeat every 10-20 seconds

**Every major file transfer app uses on-demand TCP:**
- LocalSend — HTTP REST per-file
- AirDrop — HTTPS on-demand
- Quick Share (Google) — TCP on-demand
- LANDrop — TCP on-demand
- KDE Connect — persistent, but 300s keepalive (it's a full integration platform, not just file transfer)

## Architecture Overview

```
┌─────────────────────────────────────────────────┐
│                  UDP Beacons                     │
│  (always running, every 2s, ~24 bytes each)     │
│                                                  │
│  Purpose: "I exist on this network with          │
│            this code hash"                       │
│                                                  │
│  Beacon = 8 bytes magic + 16 bytes SHA256 hash   │
└─────────────────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│              Peer Availability                   │
│                                                  │
│  Beacon received → peer_last_seen = now()        │
│  No beacon for 10s → peer unavailable            │
│  Checked every 5s by availability timer          │
│                                                  │
│  Events:                                         │
│    lan_peer_available   (first beacon seen)       │
│    lan_peer_unavailable (10s without beacon)      │
└─────────────────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────┐
│            TCP Transfer (On-Demand)              │
│                                                  │
│  User clicks send →                              │
│    1. Open TCP to peer_ip:29171                  │
│    2. Authenticate (exchange code hash)          │
│    3. Send message/files                         │
│    4. Close TCP                                  │
│                                                  │
│  No persistent connection. No ping/pong.         │
│  No keepalive. Connection lives only during      │
│  the transfer.                                   │
└─────────────────────────────────────────────────┘
```

## File Structure

```
src-tauri/src/lan/
├── mod.rs          — LanPeer (state holder), LanState (Tauri managed state)
├── protocol.rs     — Message enum, constants (ports, magic, chunk size)
├── discovery.rs    — UDP beacons + TCP listener (4 async tasks)
└── transfer.rs     — Connection struct, send/receive functions
```

## protocol.rs — Wire Format

```rust
pub const BEACON_PORT: u16 = 29170;  // UDP
pub const TCP_PORT: u16 = 29171;     // TCP
pub const BEACON_MAGIC: &[u8; 8] = b"LANDROP1";
pub const CHUNK_SIZE: usize = 262144; // 256KB

pub enum Message {
    Text { text: String },
    File { name: String, size: u64 },
    Dir { name: String },
    Batch { count: u32 },
    Done,
}
```

Messages are length-prefixed JSON: `[4 bytes big-endian length][JSON payload]`

Beacon format: `[8 bytes "LANDROP1"][16 bytes SHA256(code)[..16]]`

## discovery.rs — 4 Concurrent Tasks

### Task 1: Beacon Sender
- Fires every 2 seconds
- Sends to `255.255.255.255:29170` (limited broadcast)
- Also sends to subnet-directed broadcasts (e.g. `192.168.1.255`)
- Subnet broadcast is more reliable — many routers block limited broadcast

### Task 2: Beacon Listener
- Listens on UDP `0.0.0.0:29170`
- Ignores own beacons (checks against local IPs)
- Ignores beacons with wrong code hash
- On matching beacon: updates `peer_last_seen = (ip, Instant::now())`
- Emits `lan_peer_available` on first beacon from a peer

### Task 3: Availability Checker
- Runs every 5 seconds
- If `peer_last_seen` is older than 10 seconds → clear it, emit `lan_peer_unavailable`
- This is how the UI knows the peer went offline

### Task 4: TCP Acceptor
- Listens on TCP `0.0.0.0:29171`
- On accept: spawns `handle_incoming_session` in a new task
- Each session is independent — multiple concurrent receives are possible

### handle_incoming_session
```
1. Authenticate (exchange code hash over TCP)
2. Loop: recv_message()
   - Text → emit "lan_text_received"
   - File → receive_file() → emit "lan_files_received"
   - Batch → receive_batch() → emit "lan_files_received"
   - Done / connection closed → break
3. Connection drops automatically (Rust ownership)
```

## transfer.rs — Sending (On-Demand)

### send_text_to_peer(peer_ip, code_hash, text)
```
1. TCP connect to peer_ip:29171 (5s timeout)
2. Authenticate (send our hash, read peer hash, compare)
3. Send Text message
4. Connection drops (function returns, TcpStream dropped)
```

### send_files_to_peer(peer_ip, code_hash, paths, handle)
```
1. Walk all paths, collect (name, path) pairs
   - Directories: recursively collect files, preserve relative paths
   - Skip symlinks (security)
2. TCP connect + authenticate
3. If multiple files: send Batch { count } header
4. For each file:
   - If in subdirectory: send Dir { name } marker
   - Send File { name, size } header
   - Stream raw bytes in 256KB chunks
   - Emit progress events
5. If batch: send Done marker
6. Connection drops
```

### Receiving

`receive_file` and `receive_batch` are called by `handle_incoming_session`. Key features:
- **Path traversal protection**: sanitize filename, verify canonical path stays under output folder
- **Disk space check**: Windows `GetDiskFreeSpaceExW` before writing
- **Auto-rename**: `file.txt` → `file (1).txt` → `file (2).txt` (Windows-style deduplication)
- **Progress events**: `lan_transfer_progress` emitted during transfer

## mod.rs — State Management

```rust
pub struct LanPeer {
    handle: AppHandle,
    running: Arc<Mutex<bool>>,
    peer_last_seen: Arc<Mutex<Option<(String, Instant)>>>,
    code_hash: Arc<Mutex<Vec<u8>>>,
    out_folder: Arc<Mutex<String>>,
}
```

- `start(code, out_folder)` — hashes the code, spawns `run_discovery`
- `stop()` — sets `running = false`, clears peer
- `send_text(text)` — reads `peer_last_seen` IP, calls `send_text_to_peer`
- `send_files(paths)` — reads `peer_last_seen` IP, calls `send_files_to_peer`
- `set_out_folder(folder)` — live update without restart

No persistent connection state. `peer_last_seen` is purely informational — "do we know where the peer is?"

## TCP Authentication Handshake

Both sides must have the same connection code (entered by user). The code is SHA256-hashed, truncated to 16 bytes.

**Outgoing (initiator):**
```
1. Send our 16-byte hash
2. Read peer's 16-byte hash
3. Compare — if mismatch, drop connection
```

**Incoming (acceptor):**
```
1. Read peer's 16-byte hash
2. Send our 16-byte hash
3. Compare — if mismatch, drop connection
```

The order is reversed so both sides can verify without deadlocking.

## Frontend Events

| Event | Payload | When |
|---|---|---|
| `lan_peer_available` | `{ peer_ip }` | First beacon from matching peer |
| `lan_peer_unavailable` | `{}` | No beacon for 10s |
| `lan_text_received` | `{ text }` | Text message received |
| `lan_files_received` | `{ files, file_details }` | File(s) received |
| `lan_transfer_progress` | `{ direction, phase, ... }` | During send/receive |

## What NOT To Do

- **Don't add ping/pong** — beacons handle availability, TCP handles transfer
- **Don't keep TCP open** — open per-transfer, close after
- **Don't use aggressive timeouts** — beacons are fire-and-forget, TCP transfers have their own natural timeout (read/write will fail if peer dies)
- **Don't emit connect/disconnect** — emit available/unavailable based on beacons
- **Don't use `data-tauri-drag-region`** for titlebar — see `tauri-custom-titlebar.md`
