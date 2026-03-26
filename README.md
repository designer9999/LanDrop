# LanDrop

Instant file and text transfer between devices on the same network. No internet, no cloud, no accounts — everything stays on your LAN.

## Features

- **Instant file transfer** — drag & drop files and folders between devices
- **Text messaging** — send quick messages alongside files
- **Auto-discovery** — devices find each other automatically via mDNS
- **Cross-platform** — Windows desktop + Android (more platforms coming)
- **Image previews** — thumbnails for images in chat and full-screen lightbox
- **System tray** — runs quietly in the background, ready when you need it
- **Global hotkeys** — system-wide shortcut to instantly attach files
- **Auto-updates** — check for updates directly from the app (Windows)
- **Privacy-first** — zero data leaves your network, ever

## Download

Get the latest release from the [Releases page](https://github.com/designer9999/LanDrop/releases).

| Platform | File |
|----------|------|
| Windows  | `LanDrop_x.x.x_x64-setup.exe` (NSIS installer) |
| Android  | `landrop-android-arm64.apk` |

### Windows

1. Download the `.exe` installer from Releases
2. Run it — installs to your user profile (no admin required)
3. LanDrop starts and discovers other devices on your network

### Android

1. Download the `.apk` from Releases
2. Enable "Install from unknown sources" if prompted
3. Open LanDrop — it finds your other devices automatically

## How It Works

1. Launch LanDrop on two or more devices connected to the same network
2. Devices discover each other automatically (mDNS/Bonjour)
3. Select a device, drop files or type a message
4. Transfer happens directly over TCP — no relay, no cloud

All transfers are local. Your files never touch the internet.

## Tech Stack

- **Frontend**: Svelte 5 with Material Design 3 (Expressive) dark theme
- **Backend**: Rust (Tauri v2)
- **Discovery**: mDNS (`mdns-sd` crate)
- **Transfer**: Direct TCP with custom binary protocol
- **Mobile**: Tauri Android with native Kotlin plugins

## Building from Source

### Prerequisites

- [Node.js](https://nodejs.org/) 20+
- [Rust](https://rustup.rs/) (stable)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

### Windows

```bash
npm install
npx tauri build
```

The installer will be at `src-tauri/target/release/bundle/nsis/`.

### Android

Requires Android SDK, NDK r27, and JDK 17+.

```bash
npx tauri android build --target aarch64 --apk
```

## Releasing

Push a version tag to trigger automated builds:

```bash
# Update version in src-tauri/tauri.conf.json
git tag v1.0.1
git push --tags
```

GitHub Actions builds the Windows installer and Android APK, creates a signed release with update metadata for the auto-updater.

## License

MIT
