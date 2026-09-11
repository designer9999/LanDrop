# LanDrop architecture, experience, and Tailscale audit

Update after installed-device investigation: the Windows polling design described
below had user/profile lifecycle side effects. Version 1.7.1 disables automatic
Windows tailnet discovery as containment. The original claim that status reads
cannot affect connection state was incorrect. See the
[incident report](tailscale-incident-2026-09.md) for evidence and remaining uncertainty.

## Assessment

LanDrop has a suitable modern desktop foundation: Tauri 2, Svelte 5 runes,
TypeScript, Vite, Tailwind CSS, and a Rust networking backend. Replacing these
frameworks would not resolve the application’s most significant weaknesses.
The priorities are an accurate live-device model, reliable route selection,
explicit delivery semantics, maintainable state boundaries, and consistent shared
controls. Tauri’s frontend guidance and Svelte’s component model support this
architecture; a server-rendered web framework is not required for this desktop
client.[^1][^2]

This assessment covers the checkout beginning at `07efc15` and the changes in this
working tree. It examines frontend state, discovery, transfer framing, filesystem
commands, capabilities, dependencies, release workflows, and the tracked Android
configuration. The focus of the implementation is Windows, macOS, and Linux.
Native multi-device Tailscale testing and Android release qualification are separate
from the local automated verification described below.

The requested device behavior is a live list: restarting the application must not
make old correspondents appear available. Conversations and device customizations
remain stored, but availability is established by current discovery. A device on
both LAN and Tailscale must appear once, and a usable LAN route takes precedence.

## Changes and remaining priorities

| Priority | Finding | Disposition |
| --- | --- | --- |
| P1 | Saved history appears as the initial device list | Changed to live peers; history remains explicitly accessible. |
| P1 | Only a presumed local IPv4 subnet is admitted | Added desktop Tailscale discovery and an admission path based on the local client’s peer inventory. |
| P1 | One address cannot represent LAN plus Tailscale | Added separate routes under one persistent LanDrop UUID. |
| P1 | Incoming activity can change the composer recipient | Preserved the selected conversation and draft. |
| P1 | A dialog-level Enter handler can confirm when Cancel has focus | Restored native button activation behavior. |
| P1 | The wire protocol has no cryptographic pairing or receiver acknowledgement | Still requires a versioned protocol project. |
| P1 | Android signing identity incident remains documented | Existing publishing gate retained; no signing migration or release performed. |
| P2 | Package and toolchain updates available | Updated compatible releases and lockfiles; compatibility exceptions documented below. |
| P2 | Repeated controls and uneven interaction targets | Refined the shared Material 3 primitives and peer controls. |
| P2 | Monolithic orchestration and shared transfer progress | Further separation recommended. |
| P2 | Full-file media loading and split persistence | Further architecture work recommended. |

“Implemented” here describes code changes, not certification of every platform or
network topology. In particular, a browser with simulated native events cannot
prove that a remote colleague’s firewall and tailnet policy permit the connection.

## Architecture and data flow

The frontend is organized into feature folders for peers, transfers, and settings,
with shared UI primitives, theme generation, persistence sanitation, utility code,
and one reactive application state object. `src/lib/api/bridge.ts` wraps native
commands and event subscriptions. This is a useful boundary: components should
consume typed operations instead of constructing native invocations throughout
the component tree.

`src/App.svelte` remains the orchestration center. It restores persisted data,
loads receive-folder settings, attaches listeners, starts discovery, handles
notifications, manages outgoing work, repairs attachment paths, and presents global
progress. These responsibilities explain its size. The next refactor should extract
cohesive controllers for application startup, transfer jobs, persistence, and
notifications while retaining the existing feature components.

Rust owns a persistent device UUID and alias, discovery tasks, TCP sessions,
receive routing, and native filesystem operations. The UUID is the device identity
used to connect conversation history with current endpoints. It is not an
authentication credential: it is published to other devices and can be copied.
The normal data path is:

```text
Current discovery → device UUID + current routes → live device selector
                                             ↓
Composer → typed native command → LAN connection, then Tailscale fallback
                                             ↓
                          existing text/file transfer protocol
                                             ↓
                         native receive events → saved conversation
```

History must not feed endpoint authority back into the network layer. An old saved
address can be reassigned to a different computer. Fresh discovery and a checked
UUID exchange reduce accidental misdelivery; cryptographic pairing is necessary
to resist an intentional impersonator.

## Desktop Tailscale behavior

### Discovery

LAN discovery uses mDNS and bounded fallback probing. Ordinary multicast discovery
does not automatically become cross-site discovery just because Tailscale is
installed. Tailscale’s own mDNS feature request remains open, so the integration
uses explicit unicast discovery through the installed client.[^3]

The desktop adapter invokes `tailscale status --json`, checks that the client is
running, and collects online IPv4 peers in Tailscale’s address range. It looks for
the platform CLI, including common Windows and macOS installation paths. The
process has a deadline and output-size limit and runs without shell interpolation.
It does not log in, change the control server, modify policy, publish a service, or
install Tailscale. The official CLI supports JSON status output and desktop
platforms; it does not provide an Android or iOS CLI.[^4]

An online Tailscale node is only a candidate. LanDrop probes its TCP service and
requires LanDrop protocol identification before adding a selectable device. A
server, printer, or workstation without LanDrop must not appear just because it
belongs to the tailnet. Both desktops need the updated LanDrop implementation for
this application-level discovery exchange.

The local Tailscale peer inventory also limits the new inbound admission path.
Simply recognizing `100.64.0.0/10` is insufficient: that range is shared address
space, not proof that an arbitrary sender is an authorized Tailscale peer. Tailnet
policy and host firewall controls remain responsible for network authorization.

### Identity and route selection

One LanDrop UUID owns a LAN endpoint and a Tailscale endpoint. The frontend receives
the preferred route, current address, and available alternate addresses. Rediscovery
updates the existing device and retains its avatar, receive-folder display, and
messages. Route labels distinguish LAN, Tailscale, and the availability of both.

Outgoing connection establishment tries LAN first. If connection or identity
negotiation fails before any application payload is sent, it can use the known
Tailscale route. An in-flight file or message must not be replayed automatically
after a partial send because the current protocol has no deduplication or resumable
transfer identifier. A failed mid-transfer attempt is reported to the application.

LanDrop’s route label describes the destination route chosen by LanDrop. It does
not claim that Tailscale has established a direct WireGuard path. Tailscale itself
can use direct, DERP-relayed, or peer-relayed connections; relayed paths can affect
large-file throughput.[^5]

### Setup with an existing colleague

Run the updated LanDrop on both desktops and keep the installed Tailscale clients
connected to the existing tailnet. Permit the intended machines to reach each
other on TCP port `29171` in the host firewalls and tailnet policy. Current Tailscale
guidance favors grants for new access-control policies; existing ACLs need not be
replaced solely for this app.[^6]

No LanDrop cloud account, coordination service, Serve endpoint, Funnel endpoint,
or public port-forward is required. The existing Tailscale network supplies
connectivity. If the “server” is a Headscale control server, the integration still
reads the standard local Tailscale client; this design is compatible in principle,
but was not verified against a live Headscale deployment here.

Both devices must be awake, online, and running LanDrop. There is no offline inbox
or store-and-forward service. Geographic distance does not change the application
protocol, but latency, upstream bandwidth, relaying, and large-file reliability
matter more than on a LAN. Discovery is periodic, so presence is eventually updated
rather than instantaneous.

The implementation is IPv4-first. IPv6-only peers and automatic Android Tailscale
discovery are not covered by this desktop integration. LAN subnet detection still
uses a `/24` assumption and should move to actual interface netmasks in a future
networking pass. This limitation particularly affects multi-interface corporate
networks, unusual subnet sizes, and routed local networks.

The wire service remains on fixed TCP port `29171`; incompatible mDNS port
advertisements are explicitly rejected. Arbitrary advertised-port support is not
part of this protocol revision.

## Peer list and conversation experience

Persisted devices are contact metadata, not proof of presence. The updated state
does not restore an active conversation merely because it was selected before
shutdown. The peer bar displays currently online devices, and the welcome state is
based on live availability. History remains reachable through an explicit history
action and returns to the same device when it is rediscovered.

When an active peer disconnects, the draft remains and the active recipient is
cleared. Sending is disabled until an available recipient is selected.
Switching to another peer automatically would be dangerous because a draft may
have been written for the previous recipient. Incoming messages likewise do not
silently change the current recipient.

Successful file-send completion now removes only the queue entries captured for
that batch. Attachments added during the send, including a removed and re-added
path, remain queued. Text typed during the send is preserved. Explicit manual
recipient changes still carry the shared composer draft; per-peer draft storage
and associating failed-send recovery with its original recipient remain follow-up
work.

The refreshed UI exposes discovery errors and Tailscale availability rather than
leaving the entire explanation in debug logs. “Tailscale available” means the local
client status can be read; it does not guarantee that a particular colleague runs
LanDrop or that a firewall permits a transfer. The empty state explains the
requirement for another running instance.

These changes address the specific startup complaint while preserving stored
messages and customizations. A fuller history feature could later provide search,
per-conversation unread counts, explicit archive/forget semantics, and a dedicated
history view. Those are product additions rather than reasons to repopulate the
live device list with old contacts.

## Material 3 and component reuse

The supplied `Material 3 Design System` folder is a collection of text guidance,
including component specifications, semantic design tokens, interaction states,
spacing, and the 2026 expressive layout guidance. It is not an importable Svelte
component package. LanDrop already has an appropriate place to implement it:
`src/lib/ui`, with generated colors in `src/lib/theme` and shared styles in
`src/app.css`.[^7]

This pass retains those primitives and improves reuse and interactions. Peer
selectors use semantic selected-state colors, route text, keyboard focus, and
usable interaction targets. Repeated icon actions use the shared control. Shared
button semantics and dialog handling preserve native keyboard activation. Reduced
motion is respected by shared styling.

Decorative icon glyphs are hidden from assistive technology so controls expose
their action labels instead of font ligature names. Tailwind source detection is
explicitly scoped to the frontend directory to avoid scanning native/generated
project files; this uses the supported `source()` configuration.[^15]
On this mounted workspace the observed production build fell from about 111
seconds to 1.56 seconds after the source scope change. This is a local measurement,
not a portable performance guarantee.

At the minimum 420 × 550 window size, the original welcome panel pushed history
controls below the visible area. The panel now adapts to short windows and has a
bounded scroll area so history and the composer stay reachable. The redundant
chat placeholder is suppressed while the discovery welcome panel is present.

The device selector is an intentional adaptation with a name and route label,
so its visible surface is larger than the canonical one-line 32dp chip. It should
not be described as an exact copy of every Material chip specification. The
supplied guidance’s distinction between a visible container and a 48dp interaction
target is useful for this desktop/touch application.

The reusable implementation rules and reference inventory are in
[Material 3 component conventions](material-3-components.md). Remaining design
work includes consistent typography and spacing roles across the older transfer
components, a component showcase with interaction states, a keyboard audit of
media overlays, and native WebView checks at minimum window size. A wholesale
visual rewrite would obscure the more urgent presence and routing changes.

## Dependency modernization

Versions were checked against the live npm registry, crates.io, and official
toolchain releases.[^16] “Latest” is qualified by compatibility: adopting a compiler
that the current language tooling cannot embed would leave the project less
maintainable, even if its version number were higher.

| Package/tool | Previous | Updated |
| --- | --- | --- |
| Node.js, supported LTS line | 24.20.0 | 24.21.0 |
| Rust toolchain | 1.98.0 | 1.98.1 |
| Material Symbols font | 0.47.0 | 0.47.2 |
| Node type definitions | 26.4.0 | 24.13.4, aligned with Node 24 |
| ESLint | 10.9.1 | 10.10.0 |
| globals | 17.11.0 | 17.12.0 |
| typescript-eslint | 8.69.0 | 8.70.0 |
| Vite | 8.2.2 | 8.3.0 |
| Vitest | 4.1.11 | 5.0.0 |
| mdns-sd declared minimum | 0.21.1 | 0.21.3 |
| tauri-plugin-opener declared minimum | 2.5.3 | 2.5.5 |
| tauri-plugin-single-instance declared minimum | 2.4.1 | 2.4.4 |
| libc declared minimum | 0.2.186 | 0.2.189 |

The npm lockfile and Cargo lockfile were refreshed; the Cargo resolution updated
37 crate versions. Some changed declared minima were already resolved to newer
versions in the previous lockfile. Tokio’s process feature supports bounded local
Tailscale CLI execution. Rust toolchain pins in CI and release workflows follow
the selected patch release.

Tauri, Svelte, Tailwind, and most other direct packages were already at current
compatible stable versions. TypeScript remains `6.0.3`: Microsoft’s TypeScript 7
announcement explicitly identifies Svelte and other embedded-language tools as
requiring TypeScript 6 until the new compiler’s programmatic API is available.
The checked Svelte and ESLint package peer constraints confirm the restriction.[^8]

The pre-existing `packageManager: pnpm@12.3.4` entry is preserved. The checked-in
lockfile, scripts, Tauri hooks, and CI still use npm. This is an unresolved tooling
inconsistency, not a completed pnpm migration. A future package-manager decision
must update installation instructions, CI caching, lockfiles, and Tauri hooks
together. Do not maintain competing npm and pnpm lockfiles indefinitely.

Android’s Gradle, Android Gradle Plugin, Kotlin, and SDK settings were inspected
but not blindly moved to new major versions. They form a coupled native toolchain
with tracked Tauri-generated code. A clean Android regeneration and native build
are required to qualify that migration. Similarly, SHA-pinned GitHub Actions were
not all moved to new major versions without validating their release behavior.
Consequently this is a latest-compatible desktop dependency update, not a claim
that every version string in the repository is the newest published number.

## Security and reliability findings

### Protocol trust and delivery

The UUID exchange checks that a connection presents the expected identifier, but
the identifier is public. It cannot establish possession of a secret or prevent
deliberate spoofing. LAN text and file contents remain unencrypted at the
application layer. Tailscale adds tunnel protection to its own route, but choosing
LAN first means transfers on that route do not inherit Tailscale encryption.

The protocol also lacks receiver consent, final receiver acknowledgement,
per-file digests, cancellation, resumable offsets, and idempotent transfer IDs.
Successfully writing to a TCP stream is not proof that the receiving application
has committed the data. The UI must avoid claiming stronger delivery semantics
than the protocol can demonstrate.

The next protocol version should use reviewed authenticated transport and explicit
pairing, capability/version negotiation, transfer identifiers, receiver approval,
integrity checks, durable final acknowledgement, cancellation, and resume. This
requires coordinated changes to both sender and receiver, compatibility tests,
and downgrade behavior. It should not be implemented as ad hoc cryptography inside
a dependency refresh.

### Filesystem and resource boundaries

Existing receive code has useful protections: bounded control frames, timeouts,
concurrency limits, checked byte totals, sanitized relative paths, partial-file
staging, and failure cleanup. The receiver checks ancestor containment to reject
pre-existing directory links escaping the selected root. These are meaningful
improvements over an unbounded raw socket/file loop.

Finalization still chooses a free filename and then renames the partial file.
An in-process mutex prevents concurrent LanDrop receives choosing the same name,
but another local process can create that name between the check and rename on
platforms where rename replaces an existing target. Use a platform-aware atomic
no-replace operation and a collision retry loop in a subsequent filesystem pass.

The custom `read_file_bytes` command reads a whole file; frontend video playback
then retains Blob-backed data. The LRU budget improves retention but does not cap
the peak allocation of a single large video. Range-capable media streaming is a
better long-term design. Android’s temporary send/history commands also move whole
byte arrays through IPC, and timestamp/name-based storage can collide.

Custom Tauri commands must enforce their own path constraints. A narrow plugin
filesystem capability does not automatically constrain arbitrary native code
inside `read_file_bytes`, preview, or shell-opening commands. Tauri’s scope guidance
explicitly places enforcement in the command implementation.[^9]

### Persistence and concurrent transfers

The frontend debounces persisted snapshots, but save failures are swallowed and
shutdown does not explicitly flush the pending frontend timer. A crash or quick
exit can therefore lose the latest state. Theme preferences, frontend history,
and Rust receive-folder settings remain distinct stores; their ownership and
migrations should stay explicit.

One global progress object is insufficient when several receives and an outgoing
transfer overlap. The backend supports multiple incoming sessions, so the UI
should eventually model transfer jobs keyed by a stable transfer ID. Independent
progress, failure, cancellation, and retry states are especially valuable for
slower remote Tailscale transfers.

### Dependency audit limitations

The updated npm audit reports no findings. Cargo audit reports no
vulnerability-class findings that cause its default failure status, but still
reports seven warnings: unmaintained transitive packages and the GTK-chain
`glib 0.18.5` unsoundness advisory `RUSTSEC-2024-0429`. An exit code of zero must
not be summarized as “no Rust security issues.” These dependencies arrive through
the current desktop framework stack and need upstream resolution or a separately
qualified framework change.[^10]

Some transitive crates remain below their independently published latest versions
because upstream dependencies pin them, including `generic-array` and older
`toml` family entries. Cargo’s final update dry run resolves no additional updates
within the current constraints. The Rust edition remains 2021: an edition migration
changes language semantics, including startup environment mutation, and is not a
prerequisite for using the current toolchain.

## Android and release readiness

The repository’s security documentation records a previously exposed Android
signing key. This audit does not claim to have rotated that key or repaired trust
for installed APKs. The existing release gate remains in place. Signing migration
requires a new trusted identity and an explicit installation/update plan; hiding
the historical key does not make it secret again.[^11]

The tracked Android project targets SDK 36, requests broad storage access, uses
a broad external FileProvider path, and starts a `dataSync` foreground service.
Android’s storage guidance limits all-files access to appropriate core use cases,
and Android 15 introduced background time budgets for `dataSync` services. These
are native lifecycle and storage architecture concerns.[^12][^13]

Current Android documentation now explicitly states that target SDK 37 introduces
mandatory local-network protection. Before raising the target, LanDrop needs the
appropriate runtime permission flow or system-mediated discovery path, including
denial/revocation behavior. The documentation advises against requesting the new
permission while still targeting SDK 36.[^14]

Desktop CI already has Windows, macOS, and Linux jobs, strict Rust checks, frontend
checks, and dependency audits. Release workflows retain SHA pins and separate
updater-signature handling. Local automated checks do not establish that every
native installer, OS-signing identity, updater, firewall prompt, or Android build
has been qualified.

## Verification and rollout

Rust verification on Linux passed 41 tests, strict Clippy with all targets and
features and warnings treated as errors, and rustfmt. Tests include actual
loopback text/file transfers, reuse of a checked connection, application discovery
identification, malformed framing, partial-file cleanup, route ordering, and stale
route-removal races. Windows and macOS native runs were not performed here.

The clean Node 24 `npm ci` install completed and reported zero audit findings.
The frontend suite passed 63 tests, Svelte/TypeScript checking with zero errors or
warnings, ESLint, Prettier, and a Node 24 production build. Chromium checks against
the built frontend used simulated Tauri IPC and events. They verified cold-start
history separation, explicit history access, Tailscale labeling, one peer with
both routes, draft/recipient preservation on incoming messages, Enter on Cancel
without saving, disappearance of offline peers with Send disabled, and visible
rescan failure. Dark/light rendering and 420px, 520px, and 1000px layouts were
inspected; minimum-size history and send controls are explicitly checked.

No live tailnet, native Windows/macOS UI, external colleague, or actual cross-site
transfer was available for this verification. Those results are not implied by
the simulated browser checks or loopback tests.
The important acceptance cases are: restart with saved history but no live peers;
rediscover the same UUID; show one peer with both routes; prefer LAN; fall back
before payload when LAN is unavailable; remove stale routes; preserve the current
draft on incoming activity; reject unrelated TCP services; and expose discovery
failures. Native acceptance additionally requires two actual desktops.

For the real-device check, run the updated app on both local desktops first, then
move one machine to a genuinely different network with Tailscale connected. Send
text both ways and transfer a file in each direction, comparing SHA-256 hashes
outside LanDrop. Reconnect the local network and confirm that the peer remains a
single device and LAN becomes preferred. Quit one instance and confirm presence
eventually disappears while its history survives.

## Sources

[^1]: Tauri, [Frontend configuration](https://v2.tauri.app/start/frontend/), accessed September 11, 2026.
[^2]: Svelte, [Overview](https://svelte.dev/docs/svelte/overview), accessed September 11, 2026.
[^3]: Tailscale maintainers, [Support mDNS for name and service resolution, issue 1013](https://github.com/tailscale/tailscale/issues/1013), accessed September 11, 2026.
[^4]: Tailscale, [Tailscale CLI](https://tailscale.com/docs/reference/tailscale-cli), last validated July 30, 2026.
[^5]: Tailscale, [Connection types](https://tailscale.com/docs/reference/connection-types), accessed September 11, 2026.
[^6]: Tailscale, [Grants](https://tailscale.com/docs/features/access-control/grants), accessed September 11, 2026.
[^7]: Supplied local reference, `Material 3 Design System`, especially `design-tokens.txt`, `interaction-states.txt`, `Components/chips.txt`, and `Components/dialogs.txt`; see [reference inventory](material-3-components.md).
[^8]: Daniel Rosenwasser, Microsoft, [Announcing TypeScript 7.0](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/), July 8, 2026.
[^9]: Tauri, [Command scopes](https://v2.tauri.app/security/scope/), accessed September 11, 2026.
[^10]: RustSec, [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html), and local Cargo audit output for the refreshed lockfile.
[^11]: Repository [SECURITY.md](../SECURITY.md) and Tauri, [Android code signing](https://v2.tauri.app/distribute/sign/android/), accessed September 11, 2026.
[^12]: Android Developers, [Manage all files on a storage device](https://developer.android.com/training/data-storage/manage-all-files), accessed September 11, 2026.
[^13]: Android Developers, [Foreground service timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout), accessed September 11, 2026.
[^14]: Android Developers, [Local network permission](https://developer.android.com/privacy-and-security/local-network-permission), updated July 13, 2026.
[^15]: Tailwind CSS, [Detecting classes in source files](https://tailwindcss.com/docs/detecting-classes-in-source-files), accessed September 11, 2026.
[^16]: Version metadata: npm registry ([Vite](https://registry.npmjs.org/vite/8.3.0), [Vitest](https://registry.npmjs.org/vitest/5.0.0), [Svelte Check](https://registry.npmjs.org/svelte-check/4.7.6)); crates.io ([mdns-sd](https://crates.io/crates/mdns-sd/0.21.3)); official [Node release index](https://nodejs.org/dist/index.json) and [Rust stable channel manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml). Registry and toolchain metadata checked September 11, 2026; exact resolved dependency graph is recorded in the repository lockfiles.
