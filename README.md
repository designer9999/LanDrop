# LanDrop

LanDrop sends files and text between devices on the same local network or, on
Windows/macOS/Linux, through your existing Tailscale network. LanDrop has no account or cloud
storage. Tailscale may relay its encrypted connections when a direct path is unavailable.

> [!IMPORTANT]
> LanDrop's current transfer protocol is plaintext and does not cryptographically
> authenticate peers. Use it only on networks and with devices you trust. See
> [Security](#security) before distributing or installing Android builds.

## Features

- Direct file, folder, and text transfer over TCP
- Automatic peer discovery with mDNS
- Desktop Tailscale discovery, with LAN preferred when both routes are available
- A live device list; saved conversations remain available through **View all history**
- Windows, macOS, Linux, and Android builds
- Transfer history, image previews, and desktop video previews
- Per-peer receive folders
- Desktop system tray, global shortcut, and signed Tauri updater artifacts
- No LanDrop telemetry, account, cloud storage, or application relay

## Download

Download builds from the [GitHub Releases page](https://github.com/designer9999/LanDrop/releases).

| Platform | Release asset |
| --- | --- |
| Windows | `LanDrop_x.x.x_x64-setup.exe` |
| Android (arm64) | `landrop-android-arm64-release.apk` |
| macOS (Apple Silicon) | `LanDrop_x.x.x_aarch64.dmg` |
| macOS (Intel) | `LanDrop_x.x.x_x64.dmg` |
| Linux | `.AppImage` or `.deb` |

The replacement Android 1.8.2 APK is built and privately signed, but publication is
on hold for runtime qualification. At the owner's request, current testing is
emulator-only; physical-device coverage is not claimed. It uses the new identity
`io.github.designer9999.landrop` and will install separately from the historical
Android app; old history will not migrate automatically. Do not assume a desktop
release includes an Android asset. See the
[Android qualification record](documentation/releases/v1.8.2-verification.md).

macOS and Windows operating-system code signing is not configured yet. The Tauri
updater signature protects updater artifacts, but it is not a substitute for Apple
notarization or Windows Authenticode.

## How it works

1. Start LanDrop on two devices connected to the same network.
2. The devices advertise and discover each other through mDNS.
3. Select a peer and send files, folders, or text.
4. LanDrop opens a direct TCP connection and writes received files to the selected
   receive folder.

Saved devices do not appear in the live peer list after a restart until rediscovered.
Messages and device customizations are preserved. LAN and Tailscale addresses for
the same LanDrop device are combined into one peer, with a visible route label.

### Using an existing Tailscale network

The published 1.8.0 release restores automatic Windows discovery using native Windows
route notifications and Tailscale's local DNS. No additional server, setup code,
cloud token, or Windows Tailscale CLI/LocalAPI request is needed. This supersedes
the 1.7.1 safety pause. See the [native design](documentation/windows-native-tailnet-discovery.md)
for supported route shapes and the [verification record](documentation/releases/v1.8.0-verification.md)
for what has actually been tested. Real two-PC worldwide and Mullvad coexistence
acceptance remain unverified; the implementation and publication are not proof of
every network scenario.

1. Run the updated LanDrop on both Windows, macOS or Linux desktops.
2. Keep both installed Tailscale clients connected to your existing tailnet.
3. Allow the intended devices to connect on TCP **29171** in your tailnet policy
   and host firewalls.
4. Select the discovered peer and send text, files, or folders as usual.

On macOS/Linux, LanDrop reads `tailscale status --json` from the installed client
and checks which peers actually run LanDrop. It issues no configuration commands
and does not require a LanDrop server. Both devices must be online and running the app;
there is no offline message queue. This integration currently discovers Tailscale
IPv4 endpoints on supported desktops; automatic Android Tailscale discovery is not included.

When both routes exist, LanDrop prefers LAN and can fall back to Tailscale during
connection establishment. Interrupted transfers are reported as failed rather
than automatically replayed. Tailscale encrypts its tunnel; the direct LAN route
still uses the plaintext LanDrop protocol described below.

See the [desktop Tailscale guide](documentation/tailscale.md) for troubleshooting,
limits, and a two-device acceptance check.

The current wire protocol is intentionally simple, but it has no pairing, encryption,
receiver approval, integrity digest, resume, or final acknowledgement. Those are
prioritized in the [technical audit](documentation/technical-audit-2026-07.md).

The [September audit](documentation/technical-audit-2026-09.md) covers current
architecture, dependencies, UX, Tailscale, and remaining work. UI changes follow
the supplied [Material 3 component conventions](documentation/material-3-components.md).

The [Windows experience report](documentation/windows-experience-research.md)
covers current Windows servicing, native actionable notifications, application
identity, and the consent-based updater. Installed Windows notifications open the
sender's conversation or the update screen. Update checks never install by
themselves; **Update now** is an explicit action and waits for transfers and
drafts to be cleared. The new desktop icon's source and regeneration instructions
are in [assets/branding](assets/branding/README.md).

## Tech stack

- Svelte 5, TypeScript 6, Vite 8, and Tailwind CSS 4
- Rust and Tauri 2
- `mdns-sd` discovery and a custom framed TCP transfer protocol
- Tauri Android with small Kotlin integrations

TypeScript 7 is deliberately not used yet. Microsoft currently directs Svelte and
other embedded-language projects to remain on TypeScript 6 until the native compiler
exposes a stable tooling API.

## Build from source

### Prerequisites

- [Node.js 24 LTS](https://nodejs.org/en/about/previous-releases)
- [Rust](https://rustup.rs/) as selected by `rust-toolchain.toml`
- The [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS

Install reproducibly and run the full local verification:

```bash
npm ci
npm run verify
npm audit --audit-level=high
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --file src-tauri/Cargo.lock
```

Build the desktop app:

```bash
npm run tauri build
```

Build an Android arm64 APK (Android SDK, NDK r27, and JDK 17 are required):

```bash
npm run tauri android build -- --target aarch64 --apk
```

## Releasing

Keep the version identical in all three manifests:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

Add release notes at `documentation/releases/vMAJOR.MINOR.PATCH.md`, then create
and push an annotated version tag after the source commit has passed CI on `main`:

```bash
git tag -a v1.7.1 -m "LanDrop v1.7.1"
git push origin v1.7.1
```

The release workflow accepts stable `vMAJOR.MINOR.PATCH` tags, verifies the versions
and source, creates a draft, serializes desktop packaging so updater metadata cannot
race, validates the updater asset set, publishes SHA-256 checksums, and publishes
only after all required jobs succeed. Configure a protected GitHub `release`
environment with required reviewers before using it.

Android publishing is disabled by default after the signing-key incident. It runs
only when the repository variable `ENABLE_ANDROID_RELEASE` is exactly `true`, after
the identity migration is complete. Signing and updater credentials must be stored
as environment secrets in the protected GitHub `release` environment, not as
repository-level secrets; see
[`android/signing/README.md`](android/signing/README.md).

## Security

The Android keystore previously committed to this public repository is compromised.
It was removed from the current tree, but remains recoverable from Git history and
must never be used again. The owner approved a fresh private key and separate
Android identity for 1.8.2. The new key is configured securely for releases;
runtime qualification remains required before Android publication.

The LAN protocol also does not yet provide cryptographic authentication or encryption.
There is no evidence from this code audit that either issue has been exploited, but
both are trust-boundary defects, not cosmetic hardening tasks.

Read [SECURITY.md](SECURITY.md) and the
[July 2026 technical audit](documentation/technical-audit-2026-07.md) for the incident
response and modernization roadmap.

## License

[MIT](LICENSE)
