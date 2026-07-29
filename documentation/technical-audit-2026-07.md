# LanDrop technical audit and modernization plan

Audit date: 2026-07-29

Audited revision: `82098d707de88ebfe5da7e3cf407185e5228c424` (`main`)

Latest published release at audit time: `v1.6.12` (`4e7b2e3…`)

## Executive summary

The folder is not an obsolete copy. Its original `HEAD`, `origin/main`, and GitHub's
live `main` were identical. `main` is one commit newer than the `v1.6.12` release.
The application was already on Tauri 2, Svelte 5, Vite 8, Tailwind CSS 4, TypeScript
6, and a recent Rust toolchain.

The age problem was mainly in lockfiles, testing, release hygiene, and architecture:

- the npm lock had four known vulnerability groups;
- the Cargo lock had nine RustSec vulnerabilities;
- GitHub's latest CI run was failing at strict Clippy;
- the public repository contained a usable Android release keystore and its password;
- the LAN protocol called a public UUID exchange “authentication,” despite providing
  no cryptographic authentication, encryption, integrity, or receiver consent;
- several reproducible history, deletion, media-cache, and failure-state bugs existed;
- Android storage and foreground-service behavior does not follow the current platform
  model;
- release jobs could concurrently mutate an already-published release;
- there were only four Rust tests and no frontend, protocol, integration, Android, or
  end-to-end tests before this audit;

The compatible dependency and lock refresh performed during this audit clears npm
audit findings and Cargo Audit's vulnerability-class findings; 17 allowed RustSec
warnings remain. It also restores a clean build/check/test baseline.
It intentionally keeps TypeScript on 6.0.3. TypeScript 7.0 is current, but Microsoft's
official guidance says Svelte and other embedded-language projects must remain on
TypeScript 6 until the native compiler exposes a stable programmatic API.

Two issues cannot be “updated away”:

1. The Android signing identity is compromised and needs an incident response plus an
   application/update migration.
2. The transfer protocol needs a versioned, paired, authenticated redesign.

Until the second item ships, LanDrop should be described as a convenient transfer tool
for trusted LANs, not a secure or private peer-to-peer channel.

## Audit method

The audit covered:

- Git history, tags, live GitHub `main`, releases, assets, and Actions runs;
- every Rust, Svelte, TypeScript, Kotlin, manifest, capability, workflow, and project
  configuration file;
- dependency resolution with npm and Cargo;
- `npm audit` and `cargo audit`;
- Svelte/TypeScript checks, production build, Rust formatting, strict Clippy, and tests;
- current Tauri, TypeScript, Svelte, Vite, Rust, Android, Node, and GitHub Actions
  guidance;
- trust boundaries for discovery, transfer, filesystem writes, WebView IPC, updates,
  signing, foreground work, and local persistence.

The checkout already contained tracked generated Android and schema modifications
before the audit. Those user changes were preserved rather than reset.

## Baseline and repository status

| Area | Before this audit | Result |
| --- | --- | --- |
| Local vs GitHub | Local `HEAD` equals live GitHub `main` | Not an old checkout |
| Latest release | `v1.6.12`, one commit behind `main` | Release is not the tip |
| Frontend build | Passed | Healthy baseline |
| Svelte/TS check | No checked-in command | Added and passing |
| Strict Clippy | Failed in `path_utils.rs` | Fixed |
| Rust tests | 4 passed | 10 pass after six focused transfer tests; broader coverage is still needed |
| npm audit | 3 high, 1 moderate | 0 after refresh |
| Cargo audit | 9 vulnerabilities | 0 after refresh |
| GitHub license detection | No `LICENSE` file | MIT file added |
| Latest GitHub CI | Failed | Cause fixed locally; the replacement workflow awaits its first CI run |

The previous production bundle contained roughly 1.17 MB of JavaScript before gzip.
After switching away from the all-language Highlight.js entry, the main JavaScript
bundle is roughly 409 KB (126 KB gzip). The Material Symbols font is still roughly
566 KB and is now the largest single asset.

## P0: Android signing identity compromise

### Finding

`android/signing/landrop-release.jks` was committed to a public repository. The release
workflow also contained its store password, key password, and alias. The key first
appeared in commit `90414b0`, so deleting it from the current branch does not make it
secret.

Anyone who obtained that material can sign an APK with the exposed LanDrop identity.
Android package-signature acceptance is a trust decision; this is therefore a release
identity compromise, not merely a leaked development password.

No evidence of malicious use was found in this source audit. Absence of evidence is
not evidence that the key remained private.

### Changes already made in the working tree

- Removed the keystore from the current tree.
- Removed hard-coded Android signing credentials from the release workflow.
- Made the workflow reconstruct a keystore only from GitHub Actions secrets.
- Made the workflow verify the resulting signer certificate against an independently
  configured expected SHA-256 fingerprint.
- Kept releases as drafts until every required platform job succeeds.

These changes prevent repeating the mistake. They do not repair trust for existing
installations or remove the key from old Git objects.

### Required incident response

1. Disable Android publishing while the migration is chosen.
2. Preserve a private incident record: affected commits, releases, certificate
   fingerprint, APK hashes, and dates.
3. Generate a new keystore offline and back it up in two separately protected places.
4. Never put the exposed key into the new CI secrets.
5. For direct GitHub APK distribution, the safest recovery is a new Android
   application ID and an explicit clean-install migration. Choose an identifier
   that does not end in `.app`: Tauri warns that the current `com.landrop.app`
   conflicts with the macOS application-bundle extension.
6. If the app is enrolled in Google Play App Signing, investigate the Play-supported
   upgrade/reset path separately; do not assume it applies to sideloaded GitHub APKs.
7. Publish a security notice with old/new package IDs, signer fingerprints, trusted
   release hashes, and clear uninstall/install instructions.
8. Rewrite public Git history to reduce casual redistribution of the keystore and
   invalidate cached CI artifacts where possible. This is cleanup, not key rotation.
9. Audit every Android artifact published after the key entered the repository.

Primary guidance: [Tauri Android signing](https://v2.tauri.app/distribute/sign/android/)
and [Android app signing](https://developer.android.com/studio/publish/app-signing).

## P0: the LAN protocol is not authenticated or encrypted

### Current protocol

Discovery publishes a device UUID through mDNS. A TCP session then exchanges the same
16-byte UUID before sending framed JSON control messages and raw file bytes.

That exchange was named authentication in code and documentation, but the UUID is
public and replayable. It proves neither device identity nor possession of a secret.
Before this audit, the sender also ignored the UUID returned by the connected host.

The receiver automatically accepts text and files from a host that passes a rough
same-LAN check. There is no:

- pairing or trusted-device record;
- authenticated key agreement;
- encryption;
- receiver accept/reject step;
- protocol magic or version negotiation;
- transfer nonce or stable transfer ID;
- per-file digest;
- final success acknowledgement;
- resume or cancellation.

Consequences include peer spoofing, content disclosure to an attacker on the network,
injection of unsolicited files/text, silent corruption, and misleading “sent”
indications.

### Compatible mitigation in this audit

The returned UUID is now compared with the selected peer UUID on outgoing sessions.
That catches accidental redirection and some unsophisticated spoofing. Because an
attacker can learn and repeat the UUID, this must not be called cryptographic
authentication. New text senders terminate with `Done`; receivers also retain the
legacy v1.6.12 text-only EOF behavior so staggered upgrades do not produce false
transfer errors.

### Protocol v2 design

A professional v2 should use:

1. A persistent Ed25519 identity key per device.
2. Explicit pairing through a QR code or short authentication string.
3. TLS 1.3 or a reviewed Noise pattern authenticated by the paired device keys.
4. Protocol magic, major/minor version, capabilities, transfer ID, and nonces.
5. Receiver metadata preview and accept/reject before any final-path write.
6. A digest for every file and a signed/authenticated final acknowledgement.
7. Atomic partial files, resume metadata, cancellation, and retry semantics.
8. Trusted-device revocation and a deliberate “always accept from this device” option.
9. Migration support so v1 and v2 peers fail clearly rather than corrupting a stream.

Do not invent custom cryptography. Use established libraries and threat-model the
pairing and downgrade behavior before implementation.

## P1: transfer robustness and denial of service

### Missing bounds and deadlines

The original implementation limited only TCP connection establishment. A peer could
stall during the UUID exchange, control frame, or file body. Each accepted connection
created another detached task.

Required controls:

- bounded concurrent incoming sessions;
- handshake, control-message, and file-idle deadlines;
- maximum control-frame and text sizes;
- bounded batch count;
- checked file/session byte totals;
- per-IP connection/rate limiting;
- maximum session duration;
- supervised tasks with cancellation.

This audit adds the first compatible layer: bounded concurrent sessions, read/write
deadlines, smaller control inputs, checked arithmetic, and explicit early-EOF errors.
The file deadline is idle-based and resets after each read, so a trickle sender can
still occupy a slot indefinitely. Per-IP rate limiting, an absolute session
deadline, and full task supervision still belong in the next phase.

### Partial files and protocol desynchronization

Originally, incoming bytes were written directly to the final visible filename. A
disconnect left a plausible-looking truncated file. Name deduplication checked
existence separately from creation, allowing a race.

The sender could also read metadata, then encounter early EOF if the source shrank.
It would move on to the next protocol frame while the receiver still interpreted
bytes as the prior file body.

The compatible hardening pass:

- treats early source EOF as a hard failure;
- uses checked byte arithmetic;
- computes chunk lengths safely on 32-bit targets;
- writes to uniquely created partial files;
- renames only after the exact announced byte count is received;
- removes partial data on failure;
- no longer automatically marks received scripts/binaries executable.

A remaining Unix race exists between selecting the final collision-free filename and
the final rename: another local process can create that name in the interval and be
overwritten. A platform-aware atomic no-replace finalization is still required.

A v2 digest and final acknowledgement are still necessary to establish end-to-end
integrity and delivery success.

### Discovery/network model

The current “same LAN” check compares the first three IPv4 octets, assuming every
network is `/24`. That is wrong for common `/16`, `/20`, `/23`, `/25`, VLAN, VPN, and
multi-interface configurations.

The fallback scan probes up to 253 addresses every 30 seconds, and normal sending uses
the fixed port even though mDNS advertises a port. The service binds broadly, lacks
IPv6, and handles network changes poorly.

Replace this with:

- OS interface addresses and actual netmasks/routes;
- mDNS as the normal path, with a bounded opt-in fallback scan;
- the port from the authenticated discovery/session record;
- support for multiple active interfaces and IPv6;
- interface/network change subscriptions;
- protocol-aware liveness checks instead of “TCP port opened.”

Android's system NSD APIs should be evaluated for mobile discovery:
[Android network service discovery](https://developer.android.com/develop/connectivity/wifi/use-nsd).

## P1: Android platform design

### Storage

The manifest requests legacy broad storage permissions, all `READ_MEDIA_*`
permissions, and `MANAGE_EXTERNAL_STORAGE`. Rust writes to a hard-coded
`/storage/emulated/0/Download/LanDrop` path.

This is not the modern Android storage model and can fail unless users manually grant
special access. `MANAGE_EXTERNAL_STORAGE` is also heavily restricted for Play
distribution.

Migration:

- use MediaStore for user-visible media;
- use Storage Access Framework and persistable URI permissions for a chosen receive
  directory;
- perform `ContentResolver` copies in Kotlin or streamed Rust/native code;
- stop moving whole files through JavaScript arrays and JSON IPC;
- narrow FileProvider paths to LanDrop-owned directories;
- remove broad storage permissions only after the replacement is complete.

Guidance: [Android storage overview](https://developer.android.com/training/data-storage/)
and [all-files access](https://developer.android.com/training/data-storage/manage-all-files).

### Foreground service

Android starts an indefinite `dataSync` foreground service whenever the activity is
created and holds a multicast lock for its lifetime. Android 15 limits background
`dataSync` foreground services to six hours in a rolling 24-hour window.

An immediate timeout callback now stops the service and releases the lock. Versioned
replacement notification channels and notification-level flags configure private
lock-screen visibility for fresh and upgraded installs; this still needs physical
device verification. The architecture also needs to change:

- receiving mode should be explicit and user-controlled;
- the service should run only for a bounded, appropriate lifecycle;
- use the platform's user-initiated transfer/background mechanisms where applicable;
- release multicast resources whenever active discovery is not required.

Guidance: [foreground-service timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout)
and [foreground-service changes](https://developer.android.com/develop/background-work/services/fgs/changes).

### Future local-network permission

LanDrop targets API 36. Android 16 offers an opt-in local-network permission model;
Android 17/API 37 is expected to enforce it for apps targeting that level. Raw TCP,
UDP/mDNS, and local discovery are directly affected.

Before moving to target 37, add a permission UX and test both broad local-network
permission and the system NSD picker path.

Guidance: [Android local-network permission](https://developer.android.com/privacy-and-security/local-network-permission).

### Generated Android project state

This Linux environment does not have the Android SDK/JDK toolchain, so the Kotlin and
Gradle project was not compiled here. Tauri regenerates ignored files such as
`tauri.settings.gradle`, `tauri.build.gradle.kts`, and `tauri.properties` during an
Android CLI build; stale copies from an earlier Windows build are therefore neither
portable nor authoritative.

Before the next Android release, regenerate the Android project with Tauri 2.11 on a
clean branch, diff the tracked Gradle/template files, and deliberately reapply the
custom manifest, FileProvider, resolver, activity, foreground-service, and launcher
changes. Then run Android lint, unit tests, and an arm64 physical-device test. Do not
attempt a direct Gradle build from old generated metadata.

### Native media code

Other issues to address:

- thumbnail generation reads complete content URIs before sampling;
- an apparently unused resolver joins provider display names without robust path
  sanitation;
- `saveToDownloads` only requests media scanning despite its name;
- FileProvider currently exposes all external storage;
- Android sends duplicate complete byte arrays through JavaScript/Rust IPC.

Move these operations behind a small streamed native storage service and test them
with multi-gigabyte videos, hostile display names, revoked URI permissions, no free
space, and process death.

## P1: frontend correctness

### Fixed in this audit

- Persisted/offline peers and their chat history are selectable after restart.
- The selected history peer is no longer discarded merely because it is offline.
- “Delete older than 7/30 days” is scoped to the active peer instead of every peer.
- The `slice(-0)` history-pruning case no longer keeps every unstarred message.
- History thumbnail candidates are bounded to stop the reactive 200-entry
  eviction/reload loop; current composer attachments are intentionally included,
  and disposable Blob URLs are cleaned up.
- The stale per-peer receive-folder snapshot is kept synchronized.
- LAN event listeners are installed before discovery starts, preventing early events
  from being lost.
- Listener startup/teardown handles asynchronous mounting safely.
- A false or failed send is visible, and failed text is restored to the composer.
- Android send-cache cleanup runs in `finally`.
- Highlight.js uses a smaller common-language entry rather than the all-language
  catalog.
- TypeScript strict mode and `svelte-check` are enabled.

### Media architecture still needs replacement

Video playback currently reads an entire file into Rust/JavaScript memory, copies it
again into an `ArrayBuffer`, creates a Blob, and retains it. Android image and file
paths have similar full-buffer flows. This does not scale to large media.

Use a tightly scoped custom/asset protocol with range requests, or native platform
file descriptors/streams. Do not solve this by granting a global asset-protocol
filesystem scope. Build a reference-counted, size-aware media URL pool and render only
viewport-near messages.

Relevant Tauri APIs:
[convertFileSrc](https://v2.tauri.app/reference/javascript/api/namespacecore/#convertfilesrc)
and [raw response wrapping](https://v2.tauri.app/develop/calling-rust/#response-wrapping).

Desktop video lightbox behavior also needs a separate media model: the current
lightbox is opened only when an image source exists, while videos have no image
thumbnail source.

### Persistence and state

State is split across Tauri Store, legacy `localStorage`, theme storage, and a separate
Rust JSON receive-folder file. Effects reconcile these stores in both directions and
swallow failures.

Create:

- one versioned history/settings schema;
- runtime validation at the persistence and IPC boundaries;
- explicit migrations;
- a serialized save queue with retry and flush-on-exit;
- a backend batch migration for old attachment paths;
- typed message states: pending, sending, sent, delivered, failed, cancelled;
- transfer jobs keyed by transfer ID rather than one global progress value.

“Forget device” also needs precise semantics. Today, saved messages can recreate the
device later. The UI should state whether forgetting removes only trust/settings or
also history and cached files.

### Accessibility

The frontend contained 34 accessibility-warning suppressions. Priority work:

- replace clickable `div` elements with buttons/links or add complete keyboard
  semantics;
- give switches, sliders, icon controls, and progress indicators accessible names;
- make preview/lightbox overlays real dialogs with focus trap, Escape, return focus,
  and inert background;
- keep destructive controls available without hover;
- add `prefers-reduced-motion`;
- support dynamic viewport units and safe-area insets on mobile;
- increase small text and touch targets;
- test with keyboard-only navigation, screen readers, high contrast, and 200% zoom.

The generic dialog should use form submission instead of turning Enter on nearly any
element into confirmation.

## P1: Tauri/WebView trust boundary

The existing CSP was a good start and is now tightened with `object-src 'none'`,
`base-uri 'none'`, `frame-ancestors 'none'`, and `form-action 'none'`.

The capability file remains broader than necessary:

- shared across desktop and Android;
- redundant `*:default` plus explicit grants;
- frontend filesystem access includes `/storage/**`;
- opener and core defaults expose more than the current UI requires;
- custom Rust commands are callable by the application WebView unless custom command
  permissions are defined.

The Rust commands can read files, open folders/URLs, save supplied bytes, and remove
cache data. The URL command is now restricted to `https://github.com`, but filesystem
commands still need backend-side authorization independent of the UI.

Next:

1. Split desktop and mobile capability files.
2. List those capability files explicitly in configuration.
3. Define an application ACL manifest and permissions for custom commands.
4. Restrict frontend file access to app cache plus paths granted by a picker/SAF.
5. Validate every IPC payload at runtime.
6. Generate or share Rust/TypeScript contracts to prevent event-schema drift.

Guidance: [Tauri capabilities](https://v2.tauri.app/security/capabilities/) and
[Tauri CSP](https://v2.tauri.app/security/csp/).

## Dependencies and language/toolchain choices

### JavaScript

| Package | Before | Updated |
| --- | ---: | ---: |
| `@tauri-apps/api` | 2.10.1 | 2.11.1 |
| `@tauri-apps/cli` | 2.11.2 | 2.11.4 |
| Svelte | 5.55.1 | 5.56.8 |
| Svelte Vite plugin | 7.0.0 | 7.2.0 |
| Vite | 8.0.5 | 8.1.5 |
| Tailwind/Vite plugin | 4.2.2 | 4.3.3 |
| TypeScript | 6.0.2 | 6.0.3 |
| Roboto variable fonts | 5.2.8 | 5.3.0 |
| Material Symbols | 0.44.0 | 0.45.9 |

Tauri plugin packages were updated to their current compatible versions and pinned.
`svelte-check` 4.7.4 was added.

### Rust

The lock refresh moves the resolved Tauri core from 2.10.3 to 2.11.5 and refreshes
Tokio, Serde, `rustls-webpki`, `quick-xml`, and the plugin tree. Direct out-of-range
upgrades tested here include:

- `base64` 0.22 to 0.23;
- `mdns-sd` 0.19 to 0.20;
- `window-vibrancy` 0.7 to 0.8.

`cargo audit` now reports zero vulnerabilities and 17 informational warnings:
16 unmaintained advisories in the GTK3, `proc-macro-error`, `unic`, and `urlpattern`
transitive trees, plus one `glib` unsoundness advisory inherited from
Tauri/WebKitGTK. They cannot all be removed solely from this application.

`cargo update --dry-run` reports no compatible lockfile updates. Tauri still pins the
transitive `tao` runtime to 0.35.3 while 0.36 exists; it is not a direct application
dependency and should move with a tested Tauri runtime update.

### TypeScript 7

Do not switch this Svelte app to TypeScript 7.0 yet. TypeScript 7 is a native compiler
rewrite with major performance benefits, but its 7.0 programmatic API is not stable
for embedded-language tooling. Microsoft's release guidance explicitly names Svelte
among the projects that should remain on TypeScript 6 for now.

Re-evaluate at TypeScript 7.1 or when Svelte officially supports the native tooling
API. If CLI performance is worth investigating sooner, test a side-by-side compiler
in a separate branch without replacing the language service used by Svelte.

Primary source: [TypeScript 7 announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/).

### Runtime pins

- Node is pinned to 24.18.0 LTS.
- Rust is pinned to current stable 1.97.1 with Clippy and rustfmt.
- Cargo declares Rust 1.97 as its minimum supported version.
- Rust edition stays at 2021.

Node 20 is end-of-life. Edition 2021 is not deprecated. Move to Rust edition 2024 as
a separate change using `cargo fix --edition`; the current environment mutations in
startup need manual review because `std::env::set_var` is unsafe in edition 2024.

Guidance: [Node release schedule](https://nodejs.org/en/about/previous-releases),
[Cargo `rust-version`](https://doc.rust-lang.org/cargo/reference/rust-version.html),
and [Rust edition migration](https://doc.rust-lang.org/stable/edition-guide/editions/transitioning-an-existing-project-to-a-new-edition.html).

## CI, release, and distribution

### Improvements in this audit

- CI now runs Svelte/TypeScript checks and the frontend build.
- npm and Cargo vulnerability audits are gates.
- strict Clippy and Rust tests run across Windows, macOS, and Linux.
- Node and action inputs are pinned.
- third-party Actions are pinned to immutable commit SHAs.
- Dependabot covers npm, Cargo, Gradle, and GitHub Actions.
- CI validates the tracked Gradle wrapper, and its 8.14.3 distribution has an
  official SHA-256 pin.
- only stable `vMAJOR.MINOR.PATCH` tags are accepted, and tags are rejected when
  package, Cargo, Tauri, and tag versions differ.
- release source is verified before packaging.
- a draft is prepared first and is published only after all required builds succeed.
- desktop packagers are serialized so concurrent jobs cannot overwrite updater JSON.
- publication validates the expected desktop/updater asset set and updater platform
  URLs/signatures against the downloaded artifacts, then uploads a `SHA256SUMS`
  manifest.
- GitHub determines the latest stable release automatically from date/version rather
  than every completed workflow unconditionally overwriting the “Latest” marker.
- release tags must resolve to commits reachable from `origin/main`.
- signing jobs use a `release` environment that should be configured with required
  reviewers; updater and Android signing values belong in environment secrets, with
  repository rulesets separately protecting `v*` tags.
- Android signing uses secrets and verifies the signer certificate.
- Android publishing is disabled unless `ENABLE_ANDROID_RELEASE=true`, so desktop
  releases are not blocked while the identity migration is pending.
- Windows firewall rules are no longer recreated silently on every launch and are
  restricted to the private profile and LanDrop/mDNS ports.

### Remaining distribution work

- Apple Developer ID signing and notarization;
- Windows Authenticode signing;
- Android signing-identity migration;
- Android ABI coverage beyond arm64 or a documented arm64-only decision;
- Android lint/instrumentation tests in CI;
- release provenance/SBOM and independently published checksums;
- changelog generation and human-reviewed release notes;
- enable GitHub Private Vulnerability Reporting and configure the protected `release`
  environment in repository settings.

Follow [GitHub Actions hardening](https://docs.github.com/en/code-security/tutorials/secure-your-organization/protect-against-threats)
and [Dependabot configuration](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/configuring-dependabot-version-updates).

## Architecture recommendation

The app has useful feature folders and a centralized bridge, but several files combine
too many responsibilities:

- Rust commands, discovery, and transfer modules approach or exceed 900 lines;
- `App.svelte`, Settings, MessageBubble, ChatArea, and Composer are each hundreds of
  lines;
- one global state object combines peers, messages, files, transfers, settings, and
  persistence.

Suggested boundaries:

```text
AppBootstrap
├── PlatformCapabilities
├── PeerRepository
│   ├── DiscoveryService
│   └── TrustStore
├── TransferManager
│   ├── ProtocolV1Adapter
│   ├── ProtocolV2
│   └── TransferJobStore
├── HistoryRepository
│   ├── SchemaValidation
│   └── Migrations
├── StorageService
│   ├── DesktopPaths
│   └── AndroidMediaStoreSAF
└── MediaUrlPool
```

Repositories/services should be injected through Svelte context so UI state can be
tested without a live Tauri runtime. Validate invoke/event payloads at the bridge.

## Test strategy

### Unit tests

- protocol frame length, invalid JSON, unknown version/type, and boundary values;
- path normalization on Windows/Unix/Android-style paths;
- filename collisions, reserved Windows names, symlinks, and traversal attempts;
- state migrations and malformed persisted data;
- retention scoped to one peer;
- message state transitions and progress aggregation;
- media-cache eviction and Blob URL lifetime.

### Rust integration/property tests

- loopback sender/receiver for text, empty file, folders, large files, and concurrency;
- arbitrary chunk boundaries and partial TCP reads;
- source truncation/growth;
- disconnect at every protocol phase;
- slowloris deadlines and session semaphore;
- simultaneous same-name receives;
- disk-full/permission-denied behavior;
- malformed frames with property/fuzz testing;
- v1/v2 negotiation and downgrade rejection.

### UI and platform tests

- mock Tauri invoke/listen for component and state tests;
- Playwright/WebDriver happy paths on desktop;
- keyboard/screen-reader accessibility;
- Android SAF/MediaStore instrumentation tests;
- real Windows firewall/private-network behavior;
- macOS Intel/Apple Silicon signed/notarized artifacts;
- interrupted updater and signature-failure cases.

Tauri references:
[frontend mocking](https://v2.tauri.app/develop/tests/mocking/) and
[WebDriver testing](https://v2.tauri.app/develop/tests/webdriver/).

## Prioritized roadmap

### Phase 0 — before another Android release

- Complete the signing-key incident response.
- Choose a new application ID or a verified platform-supported migration.
- Publish the security notice and trusted fingerprints/hashes.
- Do not reuse the exposed key.

### Phase 1 — merge the compatible hardening baseline

- Review the dependency/lock updates and current source changes.
- Run CI on Windows, macOS, and Linux.
- Run an Android arm64 device test for discovery, foreground timeout, send, receive,
  previews, and storage permissions.
- Confirm the Windows installer firewall behavior.
- Release desktop builds first if Android identity migration is not ready.

### Phase 2 — correctness and scale

- Stream Android files and desktop media; remove full-buffer IPC.
- Implement atomic storage/error handling and comprehensive transfer tests.
- Replace `/24` scanning and fixed-port assumptions.
- Refactor persistence into one validated versioned store.
- Add transfer IDs, cancellation, retry, and typed failure states.
- Complete accessibility and viewport work.

### Phase 3 — protocol v2

- Publish a threat model and protocol specification.
- Add device keys, explicit pairing, authenticated encryption, consent, digests, ACK,
  cancellation, and resume.
- Support a clear v1 migration window without silent downgrade.
- Commission an independent security review before calling the protocol secure.

### Phase 4 — professional distribution

- Apple signing/notarization and Windows Authenticode.
- SBOM, checksums, provenance, and reproducible release notes.
- Stable support policy, changelog, contribution guide, and release checklist.
- Performance budgets and bundle/code splitting.

## Verification commands

Run from the repository root:

```bash
npm ci
npm run check
npm run build
npm audit --audit-level=high

cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --file src-tauri/Cargo.lock
```

Passing these commands is necessary but not sufficient. This Linux audit environment
cannot replace real Windows, macOS, and Android runtime and installer tests.

## Primary documentation consulted

- [Tauri dependency updates](https://v2.tauri.app/develop/updating-dependencies/)
- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri CSP](https://v2.tauri.app/security/csp/)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Tauri Android signing](https://v2.tauri.app/distribute/sign/android/)
- [Svelte TypeScript and `svelte-check`](https://svelte.dev/docs/svelte/typescript)
- [Vite TypeScript behavior](https://vite.dev/guide/features.html)
- [TypeScript 7 announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)
- [Android storage](https://developer.android.com/training/data-storage/)
- [Android foreground-service timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout)
- [Android local-network permission](https://developer.android.com/privacy-and-security/local-network-permission)
- [Android NSD](https://developer.android.com/develop/connectivity/wifi/use-nsd)
- [Node release schedule](https://nodejs.org/en/about/previous-releases)
- [Cargo `rust-version`](https://doc.rust-lang.org/cargo/reference/rust-version.html)
- [GitHub Actions hardening](https://docs.github.com/en/code-security/tutorials/secure-your-organization/protect-against-threats)
