# LanDrop Android — Implementation Notes

## Architecture

Tauri v2 app with Svelte 5 frontend, Rust backend, and a Kotlin plugin for Android-specific APIs.

```
Frontend (Svelte/TS)  →  Rust Backend (Tauri commands)
                      →  Kotlin Plugin (Android-only APIs)
                      →  @tauri-apps/plugin-fs (file I/O on Android)
```

## Content URIs (`content://`)

Android file pickers return `content://` URIs, not filesystem paths. Rust's `std::fs` cannot read these.

### Reading content URIs
- **Use `@tauri-apps/plugin-fs`** — `readFile("content://...")` works directly
- The dialog plugin grants temporary URI permission to the FS plugin automatically
- Permissions expire when the app is closed — blob URLs from content URIs are lost on restart

### Resolving filenames from content URIs
- Kotlin plugin `getFileName` queries Android's `ContentResolver` with `OpenableColumns.DISPLAY_NAME`
- Must use `@InvokeArg` annotated classes for parameter parsing (not `JSObject`)
- Fallback: parse URI for type hints (`image:`, `video:`) and generate names like `IMG_xxx.jpg`

### Sending files from Android
Flow: pick file → get content:// URI → read via FS plugin → save to temp → send temp file via TCP
```typescript
// In lanSendFiles():
const { readFile } = await import("@tauri-apps/plugin-fs");
const data = await readFile(contentUri);  // FS plugin handles content://
const realPath = await invoke("save_temp_for_send", { name, data: Array.from(data) });
// Then send realPath via normal TCP transfer
// After send: invoke("cleanup_send_cache")
```

## Kotlin Plugin (`FileHelperPlugin.kt`)

Located at `src-tauri/gen/android/app/src/main/java/com/landrop/app/FileHelperPlugin.kt`

### Critical: Use `@InvokeArg` for parameters

**Wrong** (silently fails):
```kotlin
val args = invoke.parseArgs(JSObject::class.java)
val path = args.getString("path")
```

**Correct:**
```kotlin
@InvokeArg
internal class PathArgs {
    lateinit var path: String
}

@Command
fun myCommand(invoke: Invoke) {
    val args = invoke.parseArgs(PathArgs::class.java)
    val path = args.path
}
```

### Plugin registration (in `lib.rs`)
```rust
#[cfg(target_os = "android")]
{
    builder = builder.plugin(
        tauri::plugin::Builder::<tauri::Wry, ()>::new("file-helper")
            .setup(|_app, api| {
                api.register_android_plugin("com.landrop.app", "FileHelperPlugin")?;
                Ok(())
            })
            .build(),
    );
}
```

### Calling from JS
```typescript
invoke("plugin:file-helper|openFile", { path })
invoke("plugin:file-helper|getFileName", { uri })
```

### Available commands
| Command | Purpose | Args |
|---------|---------|------|
| `openFile` | Open file with system viewer via FileProvider | `{ path }` |
| `getFileName` | Get display name for content:// URI | `{ uri }` |
| `saveToDownloads` | Scan file with MediaStore (gallery visibility) | `{ path }` |
| `getThumbnail` | Generate base64 thumbnail from file/content URI | `{ path, maxPx }` |

### Note on plugin commands
Some plugin commands may fail with "Command not found/not allowed" if Tauri's internal command registry doesn't pick them up. Workaround: handle the operation in JS/FS plugin instead when possible.

## Image Thumbnails on Android

Rust's `image::open()` and `std::fs::metadata()` cannot access Android paths reliably. Solution:

**Use FS plugin + blob URLs:**
```typescript
async function mobileImageSrc(path: string): Promise<string | null> {
    const { readFile } = await import("@tauri-apps/plugin-fs");
    const data = await readFile(path);  // Works for both content:// and /storage/ paths
    const blob = new Blob([data], { type: "image/jpeg" });
    return URL.createObjectURL(blob);
}
```

- Works for both content:// URIs and filesystem paths
- Blob URLs are in-memory — lost on app restart (sent images show placeholders after restart)
- Received file thumbnails persist because files are at real paths that can be re-read

## File Downloads (Saving to Gallery)

Received files are stored at `/storage/emulated/0/Download/LanDrop/`. To make them appear in Gallery:

**Approach that works — FS plugin copy:**
```typescript
const { readFile, writeFile, mkdir, exists } = await import("@tauri-apps/plugin-fs");
const data = await readFile(sourcePath);
await writeFile("/storage/emulated/0/Pictures/LanDrop/" + name, data);
```

**Required FS permissions in `capabilities/default.json`:**
```json
"fs:allow-read-file",
"fs:allow-write-file",
{
    "identifier": "fs:scope",
    "allow": [
        { "path": "$DOWNLOAD/**" },
        { "path": "$PICTURE/**" },
        { "path": "/storage/**" },
        { "path": "content://**" }
    ]
}
```

**Approaches that failed:**
- `MediaScannerConnection.scanFile()` — Kotlin plugin command wasn't found by Tauri
- `convertFileSrc()` — asset protocol doesn't serve Android filesystem paths
- `MediaStore` API copy — permission issues with scoped storage

## CSP Configuration

For images to display from blob URLs and asset protocol:
```json
"security": {
    "csp": "... img-src 'self' data: blob: asset: https://asset.localhost; ..."
}
```

## Android File Paths

| What | Path |
|------|------|
| Received files | `/storage/emulated/0/Download/LanDrop/` |
| Temp send cache | `{app_cache_dir}/send_cache/` |
| Saved images | `/storage/emulated/0/Pictures/LanDrop/` |

## FileProvider Setup

Required for opening files with other apps (Android 7+ `FileUriExposedException`).

**`src-tauri/gen/android/app/src/main/res/xml/file_paths.xml`** must include:
```xml
<paths>
    <external-path name="external" path="." />
</paths>
```

## Build & Sign

```bash
# Build APK (arm64 only — 18MB vs 58MB universal)
JAVA_HOME="C:/Program Files/Java/jdk-22" npx tauri android build --apk --target aarch64

# Sign with apksigner (NOT jarsigner — Android requires v2 signing)
apksigner sign --ks landrop-debug.keystore --ks-pass pass:landrop123 \
    --ks-key-alias landrop --key-pass pass:landrop123 \
    --out landrop-signed.apk app-universal-release-unsigned.apk
```

**Key gotchas:**
- `jarsigner` only does v1 signing → "package appears to be invalid" on install
- Must use `apksigner` from Android SDK build-tools
- Changing signing key requires uninstalling the old app first
- `JAVA_HOME` must point to JDK 22, not JRE 1.8
- `--target aarch64` builds for arm64 only (18MB vs 58MB universal)

## Platform Detection

```typescript
export function isMobile(): boolean {
    return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}
```

Use this to branch between desktop (Rust commands) and mobile (Kotlin plugin / FS plugin) code paths.

## Useful Tauri Plugins for Android

| Plugin | Purpose |
|--------|---------|
| `@tauri-apps/plugin-fs` | Read/write files, handles content:// URIs |
| `@tauri-apps/plugin-dialog` | File picker (returns content:// on Android) |
| `tauri-plugin-android-fs` | Advanced: thumbnails, MediaStore, scoped storage |
| `tauri-plugin-sharetarget` | Receive files via Android share intent |
