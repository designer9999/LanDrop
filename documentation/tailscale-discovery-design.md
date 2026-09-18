# Automatic LanDrop discovery over Tailscale

## Executive assessment

LanDrop can offer a LAN-like experience between colleagues on different home
networks: open the application, see reachable participants, select one, and send
text or files. Tailscale already provides the private network transport. The
missing component is safe discovery of running LanDrop applications, not another
VPN or a replacement internet connection.

The recommended architecture is an optional, private **presence directory** on an
existing tailnet server. Each enrolled LanDrop instance maintains one
authenticated connection to that directory and receives membership changes.
The directory exchanges connection metadata, not messages or file contents.
LanDrop verifies peer reachability separately and continues to prefer a verified
LAN route over a Tailscale route. This is an architectural recommendation, not a
feature already implemented or a service already deployed.

A safe, supported, zero-setup Windows inventory interface has not been identified
in the reviewed Tailscale release and documentation. LocalAPI event subscriptions
exist, but ordinary Windows requests participate in VPN profile ownership. A
private directory avoids that interaction. The alternative is administrator-
authorized, read-only cloud device inventory, which needs credentials and still
requires application probes.

Originally researched September 11, 2026; independently rechecked by three
parallel reviews on September 18 against current official documentation and the
LanDrop 1.7.1 codebase. Tailscale v1.102.4, released September 10, remains the
latest stable Windows version in the checked release sources. Reading the local
executable's file metadata (without executing it) also identified v1.102.4.
Upstream facts, source observations and proposed engineering choices remain
separate; this review does not establish that future releases have the same
limitations.[^1][^22]

## September 18 re-review: findings and corrections

- **No Windows upgrade fixes this integration today.** The latest stable client
  still wraps ordinary LocalAPI requests in Windows user-ownership lifecycle.
  Upstream's `TestUserConnectDisconnectOnWindows` explicitly tests that opening a
  bus watch sets the user and closing it clears the user. No reviewed watch flag
  offers an ownership-free peer inventory. The September 17 changelog entry is
  a Kubernetes Operator release, not a newer Windows client.[^22][^23]
- **Official Rust integration exists, but is experimental.** `tailscale-rs`
  lists Windows x86_64, while warning against production use and describing
  unaudited cryptography, DERP-only traffic and unsupported exit-node/Mullvad
  functionality. It is not a safe replacement for the existing desktop VPN.
  Keep LanDrop's Rust networking over ordinary sockets rather than adopting this
  experimental transport for the release.[^24]
- **OAuth wording below is narrowed.** OAuth Clients use client credentials;
  the separate alpha OAuth Apps feature now supports authorization-code device
  provisioning. Its documented single-use `auth_keys:create:once` flow is not a
  verified read-only desktop inventory login flow.[^25]
- **The Windows block is not merely cosmetic.** `online_peers` returns before
  querying Tailscale; the discovery worker therefore has no authorized remote
  IPs. The incoming-session admission check rejects non-LAN connections outside
  that set, and arbitrary frontend IP hints cannot authorize remote routes.
  A replacement must change both discovery and authorization, not just render
  more chips in the peer bar.[^20]
- **Onboarding can be simpler than a new account system.** A private directory
  on an explicitly approved server can authenticate connections using server-side
  Tailscale identity, with a member policy and application-key enrollment. Each
  desktop then needs one trusted directory link, not a password database or
  per-friend addresses. Server-side identity lookup is demonstrated by official
  `tsnet` documentation; choosing an existing Linux daemon or a separate `tsnet`
  node still requires deployment-specific review and permission.[^26]

These are research and source-review results, not a deployed replacement. No
Windows Tailscale CLI/LocalAPI call or VPN configuration change was performed.

## Private connectivity is not LAN discovery

Two computers connected to the same tailnet can use their Tailscale addresses to
communicate, subject to access policy and the destination application's listener.
They need not share Wi-Fi, a physical router, or a country. However, having a
route to a computer and knowing that the computer is running LanDrop are separate
properties. Network membership alone cannot establish application availability.

LanDrop currently advertises `_landrop._tcp.local.` using multicast DNS on the
local network. That is a local announcement mechanism: it can find an application
without first knowing its address. Tailscale does not currently supply a supported
transparent equivalent for that multicast service browsing across homes; the
upstream mDNS feature request remains open. Scanning a local subnet or enabling a
subnet router does not establish a supported cross-tailnet mDNS mechanism.[^2]

MagicDNS solves a different problem. It assigns convenient DNS names to devices,
so an application can resolve a name it already knows. Its documented behavior
does not provide an enumeration of every running LanDrop service. Consequently,
replacing a numeric address with a MagicDNS name improves addressing but does not
solve first-time discovery.[^3]

There are therefore three distinct checks: a directory or discovery source must
identify a candidate; an authenticated network path must permit contact; and the
destination must answer as the expected LanDrop application. The visible peer
list should represent the last check, not merely the first. A registered or
historically contacted computer should never become a live contact just because
its address is stored.

## Windows client lifecycle and the earlier polling design

LanDrop 1.7.0 invoked `tailscale status --json` every 15 seconds. Recorded Windows
service events showed matching client-connect, profile-selection and
client-disconnect cycles while no other desktop client request was maintaining
the session. A diagnostic status invocation produced the same sequence after
LanDrop stopped. The local incident report records the observations and their
limits; it does not prove the complete cause of the original internet outage.
Connectivity subsequently recovered following a PC restart.[^20]

In v1.102.4, the Windows LocalAPI server's `addActiveHTTPRequest` wrapper assigns
the current user when the first qualifying non-SYSTEM request starts and clears
it when the last finishes. The request handler may only read data, while its
surrounding lifecycle still changes ownership.[^4] The backend can then select a
different profile and reset its networking state. This matches the observed
closed-client churn, but does not by itself explain failures while another client
holds an active request.[^5]

A persistent IPN-bus watch is more efficient than repeated snapshots, but not a
non-owning observer. The client exposes an initial status plus peer-change
notifications, and marks the watch API unstable.[^6][^7] The request remains
subject to Windows ownership rules: holding it open can prolong profile ownership,
and releasing the last active request can end that ownership. HTTP connection
reuse is not equivalent to keeping an active request, either.[^4]

Checking that the Tailscale GUI process exists before querying introduces a race
with GUI exit. Holding a watch open to keep the VPN active makes LanDrop a VPN
lifecycle participant. Neither meets the requirement that opening or closing
LanDrop leave VPN ownership alone. A SYSTEM helper is a separate privileged
deployment and security design, not a transparent repair.

LanDrop 1.7.1 therefore disables Windows CLI discovery before any executable or
LocalAPI request is made. This safeguard must remain until a replacement path is
implemented and verified. Merely making the old timer slower is insufficient.[^20]

## Discovery alternatives

| Approach | First-time automatic discovery | Additional setup | Assessment |
| --- | --- | --- | --- |
| Existing LAN mDNS | On the local network | None | Retain for LAN; not a remote tailnet directory |
| Saved Tailscale contacts | Only after learning each contact | Address or invitation per contact | Safe fallback, but does not meet automatic first-time discovery |
| LocalAPI status polling | Candidate device inventory | Existing client | Reject for the Windows ownership behavior |
| Persistent LocalAPI watch | Candidate inventory and changes | Existing client | Efficient but still participates in Windows ownership |
| OS interface and route notifications | No remote peer inventory | Native integration | Useful safe wake-up signals, not a directory |
| Read-only Tailscale cloud API | Tailnet device inventory | Administrator-approved credentials | Supported alternative, with cloud checks and app verification |
| Private LanDrop presence directory | Enrolled, running LanDrop participants | One directory deployment and one group enrollment per app | Recommended match for event-driven presence |
| Embedded `tsnet` | Separate application-owned tailnet node | Node enrollment, identity lifecycle and Go integration | Different product architecture, not reuse of the existing desktop client |
| Tailscale Services | Published service endpoints | Tagged hosts, definitions and approval | Possible hosting tool, not automatic browsing of personal desktops |

Windows interface and route notification APIs can report local networking
changes without consulting Tailscale. They are useful for debounced reconnects
after network changes. Their documented inputs and outputs do not identify
remote LanDrop applications; treating a route prefix as a list of participants
would be an unsupported inference.[^8]

For Windows reconnect wake-ups, the installed `windows` 0.62.2 crate also exposes
safe WinRT `NetworkInformation::NetworkStatusChanged` registration/removal under
the `Networking_Connectivity` feature. This avoids introducing application-written
unsafe FFI. Keep the event token in an owned lifecycle, unregister exactly once,
signal only a bounded/coalesced worker from the callback, and handle cancellation
and callbacks already in flight. It remains a wake-up signal, not a proof of
reachability. This is source-verified feasibility, not a tested integration.[^27]

The local Tailscale web interface is also not a general peer directory. In the
reviewed release, its data response describes the local device, while its
exit-node endpoint requires management authorization and filters for exit-node
capability. The fixed route allowlist does not expose arbitrary LocalAPI status
or watch requests. This conclusion concerns endpoint coverage, not an assumption
that daemon-hosted web requests necessarily have the Windows CLI lifecycle
problem.[^9]

The cloud API supports listing devices with `devices:core:read`, without requesting
device configuration permissions.[^10] Tailscale OAuth Clients use client
credentials. The separate OAuth Apps authorization-code flow currently documents
device provisioning, not the required inventory scope or a ready-made desktop
PKCE inventory login.[^25] A long-lived client secret must be protected; it must
not be embedded in a distributable application,
frontend bundle or repository.[^11] Even authorized inventory is not proof that
LanDrop is running, and administrator visibility can exceed a particular peer's
permitted connectivity.[^15]

Webhooks do not fill the presence gap. The documented event catalog includes
administrative changes such as node creation and approval, but is not a stream
of LanDrop launches and exits. Webhooks also require a receiver and administrative
configuration. They could supplement inventory maintenance, not replace
application-level presence.[^12]

`tsnet` intentionally gives an embedded application its own tailnet node and
identity.[^13] `libtailscale` adds a C interface, but the reviewed Windows-port
proposal remains open and Rust-support work remains draft; neither is evidence
of a production-ready drop-in for this Windows/Rust integration.[^21] Tailscale
Services provides stable names for published resources but requires tag-based
service hosts and administrative setup. It could host a chosen directory later,
but is not mandatory and should not silently alter existing personal PCs.[^14]

## Recommended presence-directory architecture

The following design is proposed for implementation after the deployment model
is approved. It does not require LanDrop to run Tailscale commands, access its
private state, query LocalAPI, change exit nodes, or manage DNS and firewall rules.
It uses ordinary application connections through the already configured network.

One private service runs on an existing server reachable through Tailscale.
A trusted group link supplies its endpoint and server trust information, rather
than addresses for every colleague. Prefer server-side Tailscale identity and an
explicit member policy for enrollment where supported; a short-lived invitation
is a fallback when that identity integration is unavailable, not a mandatory
second account system. Never trust a client-supplied username or forwarded
identity header without an authenticated, explicitly trusted proxy boundary.
Each enrolled desktop receives its own revocable app identity. Thereafter,
authorized members running the updated LanDrop can become discoverable without
exchanging addresses with every other participant.[^26]

Each application registers only after its receiver has successfully bound the
LanDrop listening port. One authenticated persistent connection carries an
initial membership snapshot and sequenced join, update and leave events. The
protocol must identify its version and directory generation, so clients can
detect missed events or a server restart and request a fresh snapshot instead of
merging incompatible state indefinitely.

| Relationship | Data carried | Responsibility |
| --- | --- | --- |
| Each LanDrop app ↔ private directory | Enrollment, verified endpoint metadata, presence and small heartbeats | Find group members and expire dead sessions |
| PC A ↔ PC B over verified LAN | Text and file payloads | Preferred application route when existing policy permits |
| PC A ↔ PC B over Tailscale | Text and file payloads | Remote or fallback application route |

The directory must not become an upload server or message broker as an accidental
consequence of adding discovery. It should not receive file names, message
contents or transfer bytes. It will necessarily learn participant identifiers,
endpoint metadata and connection timing; that metadata exposure must be disclosed.
Data retention should be minimized and presence should be leased, not permanently
asserted from a database row.

A directory event supplies a candidate, not a visible peer. The receiving
application must validate the endpoint, attempt the LanDrop handshake with a
deadline, and confirm the expected identity before exposing a sendable contact.
A server connection does not prove that two participants can initiate connections
to one another. Firewall or policy restrictions can permit directory access while
blocking direct application traffic.

The same device should continue to have one conversation and one peer entry when
both routes exist. A verified LAN observation may supplement a directory-derived
Tailscale endpoint. Neither route should overwrite stable identity or history.
Once a route stops being usable, routing should change without manufacturing a
second contact or discarding a draft.

If the directory becomes unavailable, LAN discovery remains independent.
Previously verified remote routes can continue to be used only while fresh
direct liveness evidence and a separate, unexpired membership authorization
lease remain valid. Cached directory membership alone must not maintain an
online badge. New remote discovery should show an unavailable state until
reconnection succeeds, without pretending that the VPN itself is necessarily
disconnected. A brief directory disconnect need not immediately interrupt an
authorized transfer, but authorization expiry still applies.

## Performance, failure detection and retry policy

Eliminating full-device polling does not eliminate every background packet. A
persistent connection needs bounded failure detection to distinguish an idle
application from a crashed computer or broken path. WebSocket Ping/Pong provides
a standard keepalive and responsiveness mechanism, while its reconnect guidance
calls for randomized delays and increasing backoff after abnormal closure.[^19]

The proposed healthy state has one directory connection per desktop, event-driven
membership updates, and small heartbeat frames. It has no periodic Tailscale
process launch, no recurring local peer-map request, and no scan of the entire
`100.64.0.0/10` address range. A heartbeat is a small check on an existing
connection, not an enumeration of machines or a failed connection attempt to
every absent friend.

An initial engineering target is one heartbeat every 30 seconds and a 90-second
lease, subject to native testing. Graceful application shutdown can publish leave
immediately; crashes and silent network loss cannot be promised instantaneous
detection. These values deliberately trade a bounded stale-presence window for
low background traffic. They are proposed LanDrop defaults, not Tailscale
requirements or measured guarantees.

Reconnection should use capped exponential backoff with jitter, one in-flight
attempt, and a cancellable deadline. A starting cap of two minutes is reasonable
to evaluate for directory failure. A genuine local network-change notification or
an explicit refresh may trigger an earlier debounced retry, but repeated refresh
clicks must not create parallel loops. Authentication rejection should pause
automatic credential attempts and present a repair action.

Remote probe concurrency should remain small. New or changed authorized records
can schedule a probe; repeated records with identical versions should be
coalesced. An offline or policy-blocked peer should back off rather than generating
a warning toast every interval. Direct reachability still needs monitoring while
a peer is displayed, because directory presence alone cannot prove a functioning
peer-to-peer path. The implementation must not introduce an all-to-all permanent
connection requirement for large groups without explicit scalability limits.

At the proposed 30-second interval, one desktop would perform 120 directory
heartbeat exchanges per hour. That is a count derived from the proposed interval,
not a byte-bandwidth measurement. Actual overhead depends on framing, TLS,
Tailscale transport and network conditions. Measurements should record idle CPU,
memory, wake-ups, bytes and process launches, instead of equating fewer API calls
with a proven battery or throughput improvement.

## Authentication and endpoint safety

Being in `100.64.0.0/10` is not, by itself, cryptographic proof of a particular
Tailscale identity. A group name is not a password, and LanDrop's current UUID
exchange is not cryptographic pairing. A new directory must not treat a supplied
UUID, alias or arbitrary JSON address as sufficient authorization to impersonate
a colleague or direct network probes.

Enrollment should produce per-device credentials or keys, stored through
appropriate desktop credential facilities, with revocation and bounded invitation
lifetime. The server must bind each session to its enrolled identity and group.
It should prevent one enrollment from overwriting another member's identity and
define what happens when the same installation connects twice.

Membership authorization needs an independently bounded lease checked by both
endpoints, not just a presence heartbeat. New directory-authorized sessions must
fail closed after that lease expires. The proposed strict policy also cancels
group transfers on delivered revocation or authorization expiry, retaining safe
partial-file cleanup. Immediate revocation cannot be guaranteed during a
directory outage: its maximum delay is the remaining authorization lease. The
lease duration and renewal behavior must be defined and tested before release;
the presence timeout is not a substitute. These group rules do not retroactively
turn the existing unpaired LAN protocol into authenticated group access.

Endpoint registration should use the observed and validated connection context,
not unrestricted client-supplied URLs. Any design involving a reverse proxy must
explicitly establish which forwarded information is trusted. Clients should
accept only the documented protocol, fixed application port and permitted address
families. They must not follow redirects to arbitrary destinations, accept
loopback/link-local endpoints from another member, or probe unrelated private
networks because a directory record requests it.

Directory authentication and application-level peer authentication are separate
requirements. The existing application identifier and UUID handshake checks
compatibility and expected identifiers, but does not prove possession of a
secret. Before claiming verified participant identity, the implementation needs
a cryptographically authenticated peer binding or an explicitly documented,
limited trust model. This is a release requirement, not a cosmetic UI improvement.

Incoming admission must be addressed at the same time as outgoing discovery. The
current receiver accepts a remote IPv4 source only when it appears in the shared
tailnet candidate set. On Windows that set is empty while CLI discovery is
disabled. Simply displaying directory entries in the UI would therefore leave
incoming remote text and files broken. The replacement must establish authorized
endpoint membership before handshakes can succeed, while still rejecting
unrelated sources and expiring revoked membership.[^20]

The September 18 code review additionally found that a cached outgoing tailnet
route is currently checked for address range and reachability, not a separate
membership lease. The replacement must revoke that route even if its old peer
still answers. Directory snapshot/admission ordering must be tested with both
startup orders and simultaneous starts so that first-contact probes cannot
deadlock behind empty admission sets. Existing IPv4-only listeners and endpoints
also require an explicit IPv4 release scope or a deliberate IPv6 implementation.

Bounded messages, bounded queues, handshake deadlines and server session limits
are necessary on both sides. Logs should report state transitions and aggregate
failures without storing credentials or full private inventories. Disabling the
integration must cancel workers and release sessions promptly; it must not change
the Tailscale client's profile, process lifetime or configuration.

## LAN preference, Mullvad and access policy

The preferred route is a reachable LAN path, not merely an address that resembles
the local subnet. If that path fails during connection establishment, the sender
can try a verified Tailscale route. Once payload delivery has begun, automatically
replaying text or a file over another connection risks duplicate delivery unless
the protocol adds acknowledgements and deduplication. Route fallback and resumable
transfer are different features.

Mullvad exit nodes are gateways for internet traffic, not LanDrop participants.
They must not be displayed or probed as potential colleagues. Tailscale also
documents that exit-node use disables local network access by default unless its
local-access preference is enabled. LanDrop must respect that existing choice:
LAN preference is conditional on LAN being permitted and reachable, and must not
silently toggle that preference.[^16]

The UI label “Tailscale” should identify the chosen application route, not promise
a direct physical tunnel. Tailscale supports direct, DERP-relayed and peer-relayed
connections, all protected end to end; performance varies with the selected
connection type and network conditions. LanDrop should not claim to know that
type from an IP address alone.[^17]

For colleagues in the same tailnet, both desktops need access to the other's
LanDrop TCP listener on port `29171` under their existing grants/ACLs and host
firewalls. Directory access is an additional, distinct path. Sharing access to
one directory does not create peer-to-peer permission. Merely installing
Tailscale with separate accounts does not place unrelated devices into the same
authorized network.[^15]

Cross-tailnet device sharing is a separate acceptance case. Shared machines are
quarantined by default: they can answer incoming connections but cannot initiate
new connections to the recipient's machines. Since LanDrop currently opens
separate outgoing connections for sends, one-way sharing is insufficient for its
symmetric workflow. Reciprocal sharing with suitable policy, same-tailnet
membership, or a future bidirectional-session protocol is needed. Shared-device
MagicDNS names must be fully qualified.[^18]

## Code integration and user experience

The current code already separates historical contacts from live availability,
merges routes by UUID, and orders known LAN routes before Tailscale routes. Those
behaviors should remain. The compact peer bar should not be redesigned merely
because a new discovery source is introduced. History remains available even
when there are no reachable devices.[^20]

| Code area | Current responsibility | Required integration |
| --- | --- | --- |
| `src-tauri/src/lan/tailscale.rs` | CLI inventory, filtering and probe backoff; Windows safety guard | Keep the guard; separate any selected replacement into its own provider |
| `src-tauri/src/lan/discovery.rs` | LAN announcements, listener admission, probes and route liveness | Merge validated directory candidates, maintain admission, cancel independent workers |
| `src-tauri/src/lan/mod.rs` | Service lifecycle and LAN-first send routing | Own integration configuration and preserve identity/history during reconfiguration |
| `src-tauri/src/lan/protocol.rs` and `transfer.rs` | UUID handshake, discovery response and payload framing | Version any authenticated extension; keep compatibility decisions explicit |
| `src/lib/api/bridge.ts` | Typed frontend/backend boundary | Expose enrollment and discovery states without exposing secrets |
| `src/lib/state/app-state.svelte.ts` | Live devices, history and drafts | Do not hydrate presence from stored directory records |
| `src/features/settings/SettingsPage.svelte` | Settings and status | Add one understandable group-connection area; keep the existing compact main UI |

The status model should distinguish not configured, connecting, connected,
temporarily unavailable and action required. A failed directory connection should
not be labelled “Tailscale disconnected” unless that fact is independently known.
Likewise, successful directory access should not be labelled “all peers available.”
The main peer bar should show only successfully verified, currently reachable
LanDrop devices.

Closing a window is not necessarily leaving the group if the app remains running
in the system tray. The experience should explain that distinction consistently
with the existing desktop behavior. Explicitly stopping discovery or quitting
the app should withdraw presence; minimizing the window should not interrupt a
file transfer or unnecessarily remove a reachable participant.

The existing implementation also contains a 30-second LAN subnet-healing scan and
a separate 15-second route monitor for known peers. These are not the disabled
Windows Tailscale CLI timer. The new directory would not automatically remove
their traffic. A performance implementation must audit these loops separately,
prefer mDNS/network-change-driven work where suitable, and retain a deliberate
recovery path for networks where multicast discovery fails.[^20]

## Verification and release criteria

The replacement must first be tested with synthetic directory events and local
protocol fixtures, without reconnecting or reconfiguring the actual VPN.
Deterministic tests should cover snapshot replacement, ordered deltas, duplicates,
generation changes, stale responses, cancellation, retry scheduling, leases and
membership revocation. Endpoint-validation tests should include hostile address
families, excessive payloads, wrong identities and unintended destinations.

Native acceptance then requires two actual desktops. A simulated browser event
or successful build cannot establish that a distant colleague can send a file
through the real Tailscale policy. Windows is the first release gate because of
the earlier incident; macOS and Linux need their own native checks rather than an
assumption that shared Rust code proves full platform behavior.

| Scenario | Required result |
| --- | --- |
| Fresh enrollment; no historical contacts | Running authorized peer appears only after its application handshake |
| Tailscale running, remote LanDrop closed | No live peer chip and no repeated warning notifications |
| Both devices on LAN and tailnet | One contact; verified LAN route preferred |
| Colleague on another home network | Tailscale route; text and files work in both directions |
| Existing Mullvad exit node selected | Internet remains usable; no VPN, DNS, route or firewall changes by LanDrop |
| Tailscale GUI exits or restarts | No LanDrop ownership of the Windows client session |
| Remote crash, sleep or network loss | Presence expires within the stated bound; drafts and history survive |
| Directory outage or restart | LAN continues; no false cached-online state; bounded reconnection and snapshot recovery |
| Peer ACL blocks TCP 29171 | Candidate never becomes a sendable live peer merely because directory access works |
| Membership or device credential revoked | Revoke on event delivery; during an outage, reject new group sessions and cancel group transfers no later than authorization expiry |
| LAN disappears during connection setup | Verified Tailscale fallback without duplicate contact or payload replay |
| Transfer interrupted after payload starts | Clear failure; no misleading delivered state or unsafe automatic replay |
| Both apps idle for an extended period | Bounded CPU/memory/traffic; zero Tailscale CLI or LocalAPI calls from the replacement |
| App restarted with the friend offline | History retained, no resurrected live contact |

File tests should compare the received bytes or SHA-256 with the original, include
large and empty files, exercise interrupted transfers, and verify that existing
destination files are not overwritten. Text tests need both directions and
reconnect scenarios. Actual receiver acknowledgement, resumable transfer and
cryptographic LAN pairing must not be advertised unless separately implemented.

The baseline safety build's existing frontend and Rust tests are documented in
the incident report. They verify containment and existing application behavior,
not this proposed directory. No test result from that baseline should be presented
as evidence that the new architecture has already passed.[^20]

## Implementation decision and delivery boundary

A directory is the recommended match for automatic application presence with low
idle work, but deploying it introduces a service to operate and a group to enroll.
The required decision is whether that additional private service may run on the
existing server. Its operating system, reachable endpoint, authentication model
and current access policy must be established before deployment. No account
credential or server change is implied by the recommendation.

If an additional service is not acceptable, the supported alternative is
explicitly authorized read-only cloud inventory, with secure credential handling,
cached candidates, modest refresh intervals and bounded application verification.
If neither service setup nor API authorization is acceptable, safe first-time
automatic discovery on the existing Windows client is not established by the
available interfaces. A manually learned contact can reconnect safely, but it
must not be relabelled as zero-setup discovery.

This document specifies the researched implementation direction. It does not
enable Windows remote discovery, install a directory, change Tailscale settings,
or establish cross-site acceptance. The 1.7.1 Windows safety guard remains in
place pending the setup decision and implementation.

## Sources

[^1]: Tailscale. [Release v1.102.4](https://github.com/tailscale/tailscale/releases/tag/v1.102.4). September 10, 2026; reviewed September 11, 2026.
[^2]: Tailscale upstream issue tracker. [Support mDNS service discovery, issue 1013](https://github.com/tailscale/tailscale/issues/1013). Opened December 13, 2020; open status and current discussion reviewed September 11, 2026. Feature-request evidence, not a promised delivery date.
[^3]: Tailscale. [MagicDNS](https://tailscale.com/docs/features/magicdns). Last validated January 5, 2026.
[^4]: Tailscale. [`ipn/ipnserver/server.go`, v1.102.4](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnserver/server.go). `addActiveHTTPRequest`, request routing and current-user lifecycle; release September 10, 2026.
[^5]: Tailscale. [`ipn/ipnlocal/local.go`, v1.102.4](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnlocal/local.go). `SetCurrentUser` and profile switching/reset implementation; release September 10, 2026.
[^6]: Tailscale. [`client/local/local.go`, v1.102.4](https://github.com/tailscale/tailscale/blob/v1.102.4/client/local/local.go). `WatchIPNBus` endpoint and stability notice; release September 10, 2026.
[^7]: Tailscale. [`ipn/backend.go`, v1.102.4](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/backend.go). Notification masks, initial status and peer changes; release September 10, 2026.
[^8]: Microsoft. [NotifyIpInterfaceChange](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-notifyipinterfacechange) and [NotifyRouteChange2](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-notifyroutechange2). Windows API documentation; reviewed September 11, 2026.
[^9]: Tailscale. [`client/web/web.go`, v1.102.4](https://github.com/tailscale/tailscale/blob/v1.102.4/client/web/web.go), and [Manage devices using the web interface](https://tailscale.com/docs/features/client/device-web-interface). Routing, authorization, `nodeData` and exit-node filtering; reviewed September 11, 2026.
[^10]: Tailscale. [Trust credentials](https://tailscale.com/docs/reference/trust-credentials). Last validated January 30, 2026; `devices:core:read` scope and permitted endpoints.
[^11]: Tailscale. [OAuth clients](https://tailscale.com/docs/features/oauth-clients). Last validated June 30, 2026; client-credentials flow and credential lifecycle.
[^12]: Tailscale. [Webhooks](https://tailscale.com/docs/features/webhooks). Last validated January 5, 2026; event catalog, endpoint and administrative prerequisites.
[^13]: Tailscale. [tsnet](https://tailscale.com/docs/features/tsnet) and [Tailscale identity](https://tailscale.com/docs/concepts/tailscale-identity). Embedded application node model; reviewed September 11, 2026.
[^14]: Tailscale. [Tailscale Services](https://tailscale.com/docs/features/tailscale-services). Last validated February 2, 2026; tagged service hosts and administrative prerequisites.
[^15]: Tailscale. [What devices can connect to or know mine?](https://tailscale.com/docs/concepts/device-visibility). Last validated January 5, 2026; tailnet boundaries and policy-dependent visibility.
[^16]: Tailscale. [Exit nodes](https://tailscale.com/docs/features/exit-nodes). Last validated December 15, 2025; local-network access behavior. [Mullvad exit nodes](https://tailscale.com/docs/features/exit-nodes/mullvad-exit-nodes), reviewed September 11, 2026.
[^17]: Tailscale. [Connection types](https://tailscale.com/docs/reference/connection-types). Last validated June 1, 2026; direct, DERP and peer-relay transport.
[^18]: Tailscale. [Share your machines with other users](https://tailscale.com/docs/features/sharing). Quarantine, reciprocal sharing and fully qualified MagicDNS names; reviewed September 11, 2026.
[^19]: I. Fette and A. Melnikov, IETF. [RFC 6455: The WebSocket Protocol](https://www.rfc-editor.org/rfc/rfc6455.html). December 2011, sections 5.5.2–5.5.3 and 7.2.3; heartbeat and reconnect mechanisms, not LanDrop-specific timing defaults.
[^20]: LanDrop local repository, commit `89c1016`, version 1.7.1. [Incident and verification record](tailscale-incident-2026-09.md), [discovery implementation](../src-tauri/src/lan/discovery.rs), [service routing](../src-tauri/src/lan/mod.rs), [Tailscale safety guard](../src-tauri/src/lan/tailscale.rs), [protocol](../src-tauri/src/lan/protocol.rs), [transfer implementation](../src-tauri/src/lan/transfer.rs), and [frontend state](../src/lib/state/app-state.svelte.ts). September 11, 2026. Local source evidence; the new commits are not asserted to be publicly published. Recovery after restart was reported during follow-up; it was not a controlled connected-Mullvad acceptance test.
[^21]: Tailscale. [libtailscale repository](https://github.com/tailscale/libtailscale), [Windows-port pull request 25](https://github.com/tailscale/libtailscale/pull/25), and [Rust-support pull request 12](https://github.com/tailscale/libtailscale/pull/12). Repository and proposal status reviewed September 11, 2026; open proposals do not establish production support.
[^22]: Tailscale. [Changelog](https://tailscale.com/changelog), [stable Windows packages](https://pkgs.tailscale.com/stable/#windows), and [latest GitHub release](https://github.com/tailscale/tailscale/releases/latest), checked September 18, 2026. Stable Windows v1.102.4; distinguish Kubernetes Operator releases from desktop releases.
[^23]: Tailscale. [`ipn/ipnserver/server_test.go`, v1.102.4](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnserver/server_test.go), `TestUserConnectDisconnectOnWindows`; [`server.go`](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/ipnserver/server.go) and [`ipn/backend.go`](https://github.com/tailscale/tailscale/blob/v1.102.4/ipn/backend.go). Rechecked September 18, 2026; request lifecycle and watch-mask semantics.
[^24]: Tailscale. [`tailscale-rs` README](https://github.com/tailscale/tailscale-rs) and [Build with Tailscale. Build on Tailscale.](https://tailscale.com/blog/easier-building-with-tailscale), reviewed September 18, 2026. Experimental support is not a production compatibility guarantee.
[^25]: Tailscale. [OAuth Apps](https://tailscale.com/docs/features/oauth-apps) and [device provisioning with OAuth Apps](https://tailscale.com/docs/features/oauth-apps/device-provisioning), reviewed September 18, 2026. Alpha authorization-code provisioning, one-use scope, no refresh token, same-tailnet restriction and client-secret exchange.
[^26]: Tailscale. [`tsnet.Server.LocalClient` identity example](https://tailscale.com/docs/reference/tsnet-server-api#serverlocalclient), reviewed September 18, 2026. Shows server-side `WhoIs` using the connection remote address; does not itself implement LanDrop enrollment, app-key proof, proxy trust or revocation.
[^27]: Microsoft. [`NetworkInformation` Rust API](https://microsoft.github.io/windows-docs-rs/doc/windows/Networking/Connectivity/struct.NetworkInformation.html) and [`NetworkStatusChanged`](https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.networkinformation.networkstatuschanged?view=winrt-26100), compared with installed `windows` 0.62.2 generated bindings September 18, 2026.
