# Tauri v2 Reference Guide

## Window Customization

### Custom Titlebar

Remove native decorations in `tauri.conf.json`:
```json
{ "app": { "windows": [{ "decorations": false }] } }
```

Required permissions in `capabilities/default.json`:
```json
"core:window:default",
"core:window:allow-start-dragging",
"core:window:allow-close",
"core:window:allow-minimize",
"core:window:allow-toggle-maximize"
```

### Drag Region

Use `data-tauri-drag-region` attribute — **must be on each child element individually**, not just the parent:
```html
<div data-tauri-drag-region class="titlebar">
  <span data-tauri-drag-region>My App</span>
  <button>Close</button> <!-- no drag attr = clickable -->
</div>
```

Alternative manual drag:
```javascript
import { getCurrentWindow } from '@tauri-apps/api/window';
const appWindow = getCurrentWindow();
titlebar.addEventListener('mousedown', () => appWindow.startDragging());
titlebar.addEventListener('dblclick', () => appWindow.toggleMaximize());
```

### Window JS API

```javascript
import { getCurrentWindow } from '@tauri-apps/api/window';
const win = getCurrentWindow();

// State
await win.isVisible(); await win.isFocused(); await win.isMaximized();

// Control
await win.show(); await win.hide(); await win.close();
await win.minimize(); await win.maximize(); await win.toggleMaximize();
await win.center(); await win.startDragging();

// Config
await win.setTitle('title'); await win.setTheme('dark');
await win.setFullscreen(true); await win.setAlwaysOnTop(true);
await win.setSize({ width: 800, height: 600 });
await win.setPosition({ x: 100, y: 100 });
await win.setResizable(true); await win.setDecorations(false);

// Events
await win.onCloseRequested(cb); await win.onFocusChanged(cb);
await win.onResized(cb); await win.onMoved(cb);
await win.onScaleChanged(cb); await win.onThemeChanged(cb);
```

---

## Security — Permissions & Capabilities

### Capabilities file (`src-tauri/capabilities/default.json`)
```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:default",
    "core:window:allow-close",
    "dialog:default",
    "shell:default",
    "clipboard-manager:default"
  ]
}
```

### Permission format
- `<plugin>:default` — default set
- `<plugin>:allow-<command>` — allow specific command
- `<plugin>:deny-<command>` — deny specific command

### Scopes (restrict resource access)
```toml
[[scope.allow]]
fs = "$HOME/*"
[[scope.deny]]
fs = "$HOME/secret"
```

Variables: `$HOME`, `$APP_DATA`, `$CACHE`, `$TEMP`

### Platform-specific capabilities
```json
{ "platforms": ["windows", "macos"], "permissions": [...] }
```

---

## IPC — Commands

### Rust definition
```rust
#[tauri::command]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
```

### Registration
```rust
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![greet])
```

### JavaScript invocation
```javascript
import { invoke } from '@tauri-apps/api/core';
const msg = await invoke<string>('greet', { name: 'World' });
```

**Important:** Rust `snake_case` args become `camelCase` in JS:
- Rust: `out_folder: String` → JS: `{ outFolder: "..." }`

### Error handling
```rust
#[tauri::command]
fn risky() -> Result<String, String> {
    Err("something failed".into())
}
```
```javascript
try { await invoke('risky'); } catch (e) { console.error(e); }
```

### Async commands
```rust
#[tauri::command]
async fn fetch_data(url: String) -> Result<String, String> {
    // Can't use &str or State<'_, T> directly in async — use owned types
    Ok("data".into())
}
```

### Accessing window/state
```rust
#[tauri::command]
async fn cmd(
    webview_window: tauri::WebviewWindow,
    state: tauri::State<'_, MyState>,
) -> Result<(), String> { Ok(()) }
```

### Streaming with Channels
```rust
#[tauri::command]
async fn upload(on_progress: tauri::ipc::Channel, file: String) -> Result<(), String> {
    on_progress.send(50).ok();
    Ok(())
}
```
```javascript
const ch = new Channel();
ch.onmessage = (pct) => console.log(pct);
await invoke('upload', { onProgress: ch, file: '/path' });
```

---

## IPC — Events

### Emit from Rust (global)
```rust
use tauri::Emitter;
app.emit("download-progress", 50).unwrap();
```

### Emit to specific window
```rust
app.emit_to("main", "event-name", payload).unwrap();
```

### Listen in JavaScript
```javascript
import { listen } from '@tauri-apps/api/event';
const unlisten = await listen('download-progress', (e) => {
    console.log(e.payload);
});
unlisten(); // cleanup
```

### Listen once
```javascript
import { once } from '@tauri-apps/api/event';
await once('event', (e) => console.log(e.payload));
```

### Emit from JavaScript
```javascript
import { emit } from '@tauri-apps/api/event';
await emit('my-event', { data: 'value' });
```

### Listen in Rust
```rust
use tauri::Listener;
app.listen("event-name", |event| {
    println!("{}", event.payload());
});
```

---

## State Management

### Setup
```rust
use std::sync::Mutex;

struct AppState { counter: u32 }

tauri::Builder::default()
    .setup(|app| {
        app.manage(Mutex::new(AppState { counter: 0 }));
        Ok(())
    })
```

### Access in commands
```rust
#[tauri::command]
fn increment(state: tauri::State<'_, Mutex<AppState>>) -> u32 {
    let mut s = state.lock().unwrap();
    s.counter += 1;
    s.counter
}
```

### Access outside commands
```rust
let state = app_handle.state::<Mutex<AppState>>();
let mut s = state.lock().unwrap();
```

**CRITICAL:** `State<'_, AppState>` vs `State<'_, Mutex<AppState>>` mismatch causes **runtime panic**, not compile error.

**Note:** `std::sync::Mutex` is fine in async code. Use `tokio::sync::Mutex` only when holding locks across `.await` points. No need to wrap in `Arc` — Tauri handles that.

---

## Configuration

### tauri.conf.json key properties
```json
{
  "productName": "My App",
  "version": "1.0.0",
  "identifier": "com.company.app",
  "app": {
    "windows": [{
      "title": "My App",
      "width": 800, "height": 600,
      "minWidth": 400, "minHeight": 300,
      "decorations": true,
      "transparent": false,
      "resizable": true
    }],
    "security": { "csp": "..." }
  },
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "bundle": {
    "active": true,
    "targets": ["nsis", "msi", "dmg", "appimage", "deb"],
    "createUpdaterArtifacts": true
  },
  "plugins": { "shell": { "open": true } }
}
```

### Platform-specific overrides
Files auto-merged: `tauri.windows.conf.json`, `tauri.macos.conf.json`, `tauri.linux.conf.json`

### Formats supported
- JSON (default)
- JSON5 (enable `config-json5` feature)
- TOML (enable `config-toml` feature)

---

## Plugin System

### Using plugins
```rust
// Cargo.toml
tauri-plugin-shell = "2"

// lib.rs
tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
```

```javascript
// npm install @tauri-apps/plugin-shell
import { open } from '@tauri-apps/plugin-shell';
```

### Creating plugins
```bash
npx @tauri-apps/cli plugin new my-plugin
```

### Plugin lifecycle hooks
- `setup` — initialization, state
- `on_webview_ready` — per-window init
- `on_navigation` — validate navigation
- `on_event` — core event loop
- `on_drop` — cleanup

### Common plugins
| Plugin | Rust crate | NPM package |
|--------|-----------|-------------|
| Dialog | `tauri-plugin-dialog` | `@tauri-apps/plugin-dialog` |
| Shell | `tauri-plugin-shell` | `@tauri-apps/plugin-shell` |
| Clipboard | `tauri-plugin-clipboard-manager` | `@tauri-apps/plugin-clipboard-manager` |
| FS | `tauri-plugin-fs` | `@tauri-apps/plugin-fs` |
| HTTP | `tauri-plugin-http` | `@tauri-apps/plugin-http` |
| Notification | `tauri-plugin-notification` | `@tauri-apps/plugin-notification` |
| Store | `tauri-plugin-store` | `@tauri-apps/plugin-store` |
| Updater | `tauri-plugin-updater` | `@tauri-apps/plugin-updater` |
| Process | `tauri-plugin-process` | `@tauri-apps/plugin-process` |
| Window State | `tauri-plugin-window-state` | — |

---

## Migration from v1

### Key changes
| v1 | v2 |
|----|-----|
| `@tauri-apps/api/tauri` | `@tauri-apps/api/core` |
| `@tauri-apps/api/window` | `@tauri-apps/api/webviewWindow` |
| `tauri::Window` | `tauri::WebviewWindow` |
| `app.get_window()` | `app.get_webview_window()` |
| `tauri.allowlist` | capabilities system |
| `SystemTray` | `tray::TrayIconBuilder` |
| `Menu::new()` | `menu::MenuBuilder::new(app)` |
| `TAURI_PRIVATE_KEY` | `TAURI_SIGNING_PRIVATE_KEY` |
| `https://tauri.localhost` | `http://tauri.localhost` |

### Core features moved to plugins
CLI, Clipboard, Dialog, FS, HTTP, Notifications, Shell, Process, Updater

### Run migration
```bash
npm install @tauri-apps/cli@latest
npm run tauri migrate
```

---

## Updater

### Key generation
```bash
npm run tauri signer generate -- -w ~/.tauri/myapp.key
```

### Config
```json
{
  "bundle": { "createUpdaterArtifacts": true },
  "plugins": {
    "updater": {
      "pubkey": "...",
      "endpoints": ["https://releases.myapp.com/{{target}}/{{arch}}/{{current_version}}"]
    }
  }
}
```

### Build env vars
```bash
export TAURI_SIGNING_PRIVATE_KEY="key_content_or_path"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="optional"
```

### Check for updates (JS)
```javascript
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

const update = await check();
if (update) {
    await update.downloadAndInstall();
    await relaunch();
}
```

### Server response format
```json
{
  "version": "2.0.0",
  "url": "https://releases.myapp.com/app_2.0.0.exe",
  "signature": "...",
  "notes": "Release notes"
}
```
Return `204 No Content` if no update available.

---

## Distribution

### Build
```bash
npm run tauri build
npm run tauri build -- --no-bundle        # build only
npm run tauri bundle -- --bundles app,dmg  # bundle only
```

### Bundle targets
- **Windows:** `nsis` (.exe), `msi`
- **macOS:** `app`, `dmg`
- **Linux:** `appimage`, `deb`, `rpm`

### Code signing
- **macOS:** Requires signing identity + notarization
- **Windows:** Certificate thumbprint or path
- **Linux:** GPG signing available
