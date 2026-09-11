# Desktop Tailscale guide

LanDrop can discover and communicate with other updated LanDrop desktops through
an existing Tailscale network. It supports Windows, macOS, and Linux with the
standard installed Tailscale client. The first implementation uses IPv4.

## Requirements

Both devices need the updated LanDrop running, a connected Tailscale client, and
permission to connect to each other on TCP port `29171`. Tailnet access policy and
each device’s firewall must allow that connection. The devices do not need to be
in the same country, on the same Wi-Fi, or logged into a new LanDrop service.

The application reads the local desktop client using `tailscale status --json`.
This is a read-only operation. It does not change login state, control-server
settings, ACLs/grants, routes, or Tailscale version. CLI location handling covers
PATH and common Windows/macOS installations. Nonstandard installations may need
the CLI made available in the graphical application’s environment.

## What appears in the app

Only a peer that passes the LanDrop service check becomes a live device. Other
tailnet devices are not contacts merely because Tailscale reports them online.
Tailscale discovery refreshes periodically, so a new device may take a short time
to appear. Use the rescan control to restart discovery when troubleshooting.

One LanDrop UUID has one device entry, even when both LAN and Tailscale addresses
are known. The label names the selected route, and the description indicates an
alternate route where available. If LAN is reachable, LanDrop prefers it. If
connection establishment fails, the sender can use the known Tailscale route.

Closing or disconnecting a peer eventually removes it from the live list. Stored
messages remain under **View all history**. If the selected peer goes offline,
the draft stays visible and sending is disabled. Both peers must be available for
delivery; this is not an offline messaging service.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Tailscale CLI unavailable | Confirm the desktop client is installed and its CLI can be found. On macOS, check the app bundle or Homebrew installation. |
| Tailscale disconnected | Connect the existing Tailscale client, then rescan. |
| Tailscale available but colleague absent | Confirm the colleague runs the updated LanDrop, not just Tailscale; check TCP 29171 policy/firewall access. |
| Two computers on LAN are not discovered | Check multicast filtering, Wi-Fi client isolation, the host firewall, and whether both addresses lie in the currently supported local subnet. |
| Remote file transfer is slow | Check the machines’ uplink speeds and `tailscale status` / `tailscale ping`; a relayed tunnel can be slower. |
| File transfer fails after starting | Check disk space and connectivity. Retry deliberately; automatic mid-stream replay/resume is not implemented. |
| Old history exists but no device chip appears | Expected: saved history is independent of current discovery. |

Do not open a public internet port or enable Funnel merely to use this integration.
Follow your existing tailnet policy. For a new policy, Tailscale recommends
[grants](https://tailscale.com/docs/features/access-control/grants). Its
[CLI reference](https://tailscale.com/docs/reference/tailscale-cli) explains status
and ping diagnostics, and [connection types](https://tailscale.com/docs/reference/connection-types)
explains direct versus relayed connections.

## Limits and trust

Tailscale protects traffic on its tunnel; direct LAN traffic still uses LanDrop’s
plaintext, unpaired protocol. Device UUID matching is not cryptographic pairing.
Use trusted LANs and deliberately scoped tailnet access. The application also
lacks final receiver acknowledgement and per-file integrity digests.

Discovery currently uses Tailscale IPv4 addresses, not IPv6-only endpoints. LAN
subnet matching still assumes `/24`. Android has no supported Tailscale CLI, so
automatic mobile Tailscale discovery needs a separate native integration.
Headscale configurations are expected to work through the standard connected
Tailscale client, but were not exercised in a live Headscale environment here.

There is no relay service implemented by LanDrop. Tailscale may use its own
encrypted relay infrastructure when a direct tunnel cannot be established. The
“Tailscale” label describes the application route, not whether the tunnel is direct.

## Two-device acceptance check

1. Open the updated apps on two desktops sharing a LAN and tailnet. Wait for
   discovery; confirm one device chip per remote UUID and a LAN route.
2. Send a short message and a file in both directions. Compare the received file’s
   SHA-256 with the original using an external checksum tool.
3. Move one desktop to a different network while retaining Tailscale. Confirm the
   peer is reachable through Tailscale; repeat both-direction messages and files.
4. Restore the LAN. Confirm the peer is still one device and LAN becomes preferred.
5. Start a draft, then close the remote LanDrop instance. Confirm the device leaves
   the live list, the draft stays intact, and Send becomes disabled.
6. Restart LanDrop with the remote instance still closed. Confirm the old peer does
   not reappear in the live list and **View all history** still contains the messages.

These checks validate the actual native clients and network policy. Automated
loopback protocol tests and simulated browser events cannot substitute for them.
