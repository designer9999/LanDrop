# LanDrop

LanDrop sends files and text directly between devices on the same local network.
There is no account, cloud storage, or relay server.

> [!IMPORTANT]
> LanDrop's current transfer protocol is plaintext and does not cryptographically
> authenticate peers. Use it only on networks and with devices you trust. See
> [Security](#security) before distributing or installing Android builds.

## Features

- Direct file, folder, and text transfer over TCP
- Automatic peer discovery with mDNS
- Windows, macOS, Linux, and Android builds
- Transfer history, image previews, and desktop video previews
- Per-peer receive folders
- Desktop system tray, global shortcut, and signed Tauri updater artifacts
- No telemetry, account, cloud storage, or internet relay

## Download

Download builds from the [GitHub Releases page](https://github.com/designer9999/LanDrop/releases).

| Platform | Release asset |
| --- | --- |
| Windows | `LanDrop_x.x.x_x64-setup.exe` |
| Android (arm64) | `landrop-android-arm64-release.apk` |
| macOS (Apple Silicon) | `LanDrop_x.x.x_aarch64.dmg` |
| macOS (Intel) | `LanDrop_x.x.x_x64.dmg` |
| Linux | `.AppImage` or `.deb` |

macOS and Windows operating-system code signing is not configured yet. The Tauri
updater signature protects updater artifacts, but it is not a substitute for Apple
notarization or Windows Authenticode.

## How it works

1. Start LanDrop on two devices connected to the same network.
2. The devices advertise and discover each other through mDNS.
3. Select a peer and send files, folders, or text.
4. LanDrop opens a direct TCP connection and writes received files to the selected
   receive folder.

The current wire protocol is intentionally simple, but it has no pairing, encryption,
receiver approval, integrity digest, resume, or final acknowledgement. Those are
prioritized in the [technical audit](documentation/technical-audit-2026-07.md).

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

Then create and push an annotated version tag:

```bash
git tag -a v1.6.13 -m "LanDrop v1.6.13"
git push origin v1.6.13
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
must never be used again. Do not publish another Android update until the application
identity and user migration plan have been chosen.

The LAN protocol also does not yet provide cryptographic authentication or encryption.
There is no evidence from this code audit that either issue has been exploited, but
both are trust-boundary defects, not cosmetic hardening tasks.

Read [SECURITY.md](SECURITY.md) and the
[July 2026 technical audit](documentation/technical-audit-2026-07.md) for the incident
response and modernization roadmap.

## License

[MIT](LICENSE)
