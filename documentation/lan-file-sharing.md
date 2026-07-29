# LAN File Sharing Architecture

LanDrop uses local-only discovery and on-demand TCP transfer. It must keep
working when multicast is unreliable, when Windows has virtual adapters, and
when VPN software such as Mullvad owns the default route.

> [!WARNING]
> This documents the current legacy v1 behavior, not a security boundary. Device
> UUIDs are public discovery identifiers, the `/24` test is only a routing
> heuristic, and transfers are not authenticated or encrypted. See
> `technical-audit-2026-07.md` for the replacement plan.

## Core Rules

- The current implementation accepts only IPv4 peers whose first three octets
  match. With a local IP of `192.168.0.98`, it assumes `192.168.0.0/24`.
  This is inaccurate on other subnet sizes and must be replaced with real
  interface/netmask information.
- mDNS is treated as a hint. The TCP UUID handshake identifies the advertised
  peer but does not authenticate it.
- Sending is on-demand. The TCP connection opens for one text/file transfer and
  closes after `Done`.
- The app must never get stuck on a stale peer IP. If a stored or UI-provided IP
  fails, the service scans the local `/24` and recovers the peer by UUID.

## Lessons From Mature LAN Apps

- LocalSend combines multicast discovery with an HTTP registration scan across
  local IPs when multicast is unsuccessful. LanDrop follows the same principle:
  mDNS is fast, but active same-LAN probing is the reliability fallback.
- KDE Connect uses UDP discovery plus TCP connections and documents that
  discovery can fail when UDP broadcast is blocked. Its issue tracker also shows
  VPN/default-route bugs where discovery packets go out the wrong interface.
- Warpinator uses zeroconf/mDNS for discovery and a direct TCP/gRPC transfer
  channel. It treats the local network and firewall surface as explicit runtime
  behavior, not incidental OS behavior.

## Runtime Flow

```
start_lan_service
  -> choose local LAN IPv4
     - prefer physical private LAN adapters
     - reject VPN and virtual adapter names
  -> register _landrop._tcp.local. with the chosen LAN IP
  -> browse mDNS for peers
  -> listen on TCP 0.0.0.0:29171
  -> run periodic same-LAN TCP healer scan
```

## Peer Discovery

### mDNS

The app registers `_landrop._tcp.local.` with:

- `id`: stable device UUID
- `alias`: user-visible device name
- `dtype`: device type
- TCP port: `29171`

When a peer resolves, the app selects only an IPv4 address from the same `/24`
as the current local LAN IP. If a peer advertises multiple IPv4 addresses, the
app logs them and stores the same-LAN address.

### Same-LAN Healer Scan

Every 30 seconds, and also after send candidates fail, LanDrop probes the local
`/24` subnet on TCP `29171`.

Each probe performs only the LanDrop UUID handshake:

1. Connect to `peer_ip:29171`.
2. Send this device's 16-byte UUID.
3. Read the remote 16-byte UUID.
4. If the UUID matches the target peer, store that IP and emit discovery.

This makes send-back recover even when mDNS disappears or the peer changed IP.
Because UUIDs are public, it does not protect against a malicious LAN host.

## Send Path

`send_text` and `send_files` build candidate IPs from backend discovery plus the
UI hint. Before attempting transfer, each candidate must pass
`is_current_lan_peer_ip`.

If all candidates fail:

1. Scan the local `/24`.
2. Recover the peer by UUID.
3. Retry the transfer using the recovered IP.
4. Remember the working IP for future sends.

## Receive Path

Incoming TCP sessions are rejected unless the remote address passes the current
same-`/24` heuristic. This may filter some virtual interfaces, but it neither
models arbitrary subnet boundaries nor proves that a host is trusted.

After the incoming UUID exchange succeeds, the sender is registered or updated
in `discovered_peers`, so a device that can send to us can also be sent back to
using the exact same LAN source address.

## Firewall Surface

- TCP `29171`: direct text/file transfer and UUID probes.
- mDNS UDP `5353`: zeroconf discovery used by the `mdns-sd` crate.

Windows installer hooks add inbound TCP/UDP rules for the installed
`landrop.exe`. If running a different executable path during development,
Windows may need a separate firewall allow rule for that path.

## What Not To Do

- Do not trust the first IPv4 address from mDNS.
- Do not accept `10.x`, `172.x`, or `192.168.x` blindly; same subnet matters.
- Do not persist a failed VPN or virtual adapter IP as the peer's working IP.
- Do not rely on mDNS as the only discovery mechanism.
- Do not keep persistent TCP connections or ping/pong keepalives for transfers.
