# Server-free Windows Tailscale discovery

Decision date: September 18, 2026. Implementation base: `41bed30`.
Requirement: open LanDrop on desktops already connected to the same tailnet;
discover running apps without a second server, setup code, token or VPN change.

## Corrected decision

The earlier private-directory prototype did not meet this requirement. It was
archived recoverably in Git stash (`Archive unshipped private-directory prototype`)
and its local installer moved outside the release-output directory. It was not
installed, pushed, deployed or published. Its test counts do not validate this
different implementation.

The earlier research also dismissed OS routes too broadly. A route is not proof
of a running application, but **can provide discovery candidates**, just as a LAN
address scan does. Tailscale v1.102.4 normally installs per-peer IPv4 `/32` routes
on Windows. Native route notifications and bounded application probes can reuse
those candidates without accessing Tailscale's Windows control API.

The LocalAPI risk remains conditional: a read request does not always change the
VPN. Ownership changes occur at active-request transitions; network reset depends
on profile selection. Existing GUI watches or unattended profiles can avoid a
reset. This implementation avoids that lifecycle entirely rather than relying on
those circumstances. The earlier internet outage's complete cause remains
unproven; it must not be described as conclusively caused by every status read.

## Evidence from this workstation

Read-only Windows networking commands observed the active **Tailscale Tunnel**,
interface index 2, GUID `{37217669-42DA-4657-A55B-0D995D328250}`. Its table contained
540 CGNAT `/32` routes and no CGNAT `/10`. The two Tailscale IPv6 `/128` routes were
the local address and DNS service, not remote peers. No Tailscale CLI or LocalAPI
call was made.

Three sampled remote IPv4 host routes were reverse-resolved directly through
Tailscale's local DNS address `100.100.100.100`; all returned a name ending in
`.mullvad.ts.net`. No LanDrop TCP probes were sent to these servers. PowerShell
reported 520 ms for the first lookup and 0 ms for the following two, at its timer
resolution. This tiny sample includes tool initialization and is **not** an
application performance benchmark or a promise that every lookup is instantaneous.

The subsequently implemented native Rust provider passed a full isolated
lifecycle test on this PC: 538 remote candidates after self/service exclusions,
533 classified Mullvad, five nonservice, zero unresolved; 367 ms for one run with
10 ms completion sampling. It made no remote TCP connections. Cancellation
unregistered the owned observer and cleared cached route authority. This proves
local provider execution, not which remote devices run LanDrop or two-PC delivery.

## Implementation contract

- Read adapter identity/state and route tables using safe Rust wrappers over
  Windows networking APIs. Disable netdev's optional gateway feature to avoid
  unrelated ARP probes. Never use the route library's write operations.
- Subscribe before taking the first snapshot. Treat notifications as coalesced
  invalidation signals and reconcile the full table, not unreliable event deltas.
  Own and unregister the watcher with discovery shutdown.
- Accept only exact CGNAT `/32` routes on the verified active Tailscale adapter;
  exclude the local address and Quad100. Do not expand aggregate prefixes or scan
  the complete CGNAT range. A route remains a candidate, not a visible contact.
- Bound and cache direct PTR classification through Quad100. No public/system DNS
  fallback. Match Mullvad by exact DNS-label suffix, never an arbitrary substring.
  DNS failure is not proof of a service node, identity or app availability.
- Bound failed/unknown candidate work. Newly discovered routes should not wait
  behind hundreds of cached service nodes. Existing peers opening LanDrop may not
  cause a route change, so application-level retry is still necessary.
- Publish live devices only after the existing LanDrop application handshake.
  Remove lost Tailscale routes without removing a still-working LAN route or saved
  history. Preserve LAN-first connection selection and the approved compact UI.
- Scope Windows outgoing tailnet sockets to the current Tailscale source address;
  check generation before/after connection and around payload I/O. Incoming
  CGNAT sockets must target the current Tailscale local address, not the LAN IP.
  These checks do not add cryptographic authentication to the legacy UUID framing.

## Limits and acceptance

The adapter GUID and route shape are version-matched upstream implementation
details, not a permanent Tailscale inventory API contract. Explicit OneCGNAT
configuration or more than 10,000 CGNAT peers can aggregate IPv4 routes; discovery
must fail clearly rather than scan a `/10` or resume unsafe CLI polling. IPv6
peer routes are aggregated into a `/48`, so this does not solve IPv6-only discovery.

Source binding relies on normal Windows strong-host networking. Unusual weak-host
or administrator-modified routes need separate qualification. The LAN protocol
is still unpaired/plaintext; Tailscale encrypts its tunnel, not the LAN fallback.
Use trusted LANs and appropriately restricted tailnet permissions.

Real two-PC tests remain necessary: both startup orders, a peer opening the app
after long inactivity, LAN preference, remote messages/files, disconnect/reconnect,
Mullvad coexistence, and opening/closing both applications. Existing firewall and
tailnet policy must allow TCP 29171. No such settings are changed by discovery.

## Primary sources checked September 18

- [Tailscale route selection](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnlocal/local.go): `shouldUseOneCGNATRoute`.
- [Tailscale route aggregation](https://github.com/tailscale/tailscale/blob/v1.102.4/net/routemanager/routemanager.go): CGNAT threshold and IPv6 coarsening.
- [Windows adapter identity](https://github.com/tailscale/tailscale/blob/v1.102.4/net/tstun/tun_windows.go).
- [LocalAPI request lifecycle](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnserver/server.go).
- [Tailscale reverse DNS](https://github.com/tailscale/tailscale/blob/v1.102.4/net/dns/resolver/tsdns.go) and [node-name lookup](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnlocal/node_backend.go).
- [Mullvad exit nodes](https://tailscale.com/docs/features/exit-nodes/mullvad-exit-nodes) and [Quad100](https://tailscale.com/docs/reference/quad100).
- [Windows route snapshots](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-getipforwardtable2) and [route notifications](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-notifyroutechange2).
- [net-route](https://docs.rs/net-route/0.4.6/net_route/) and [netdev](https://docs.rs/netdev/0.46.3/netdev/): version-matched safe wrappers; dependency internals use FFI, no new application-written unsafe Rust.
