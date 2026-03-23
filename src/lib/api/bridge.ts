import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export interface StatusResponse {
  ok: boolean;
  app_version: string;
  local_ip: string;
}

export interface FileInfo {
  name: string;
  size: string;
  type: string;
  count?: number;
}

export interface DeviceIdentity {
  id: string;
  alias: string;
  device_type: string;
}

export interface DiscoveredPeer {
  id: string;
  alias: string;
  device_type: string;
  ip: string;
  port: number;
}

export async function getStatus(): Promise<StatusResponse> {
  return invoke<StatusResponse>("get_status");
}

// ── LAN Service (zero-config mDNS) ──

export async function startLanService(): Promise<void> {
  return invoke("start_lan_service");
}

export async function stopLanService(): Promise<void> {
  return invoke("stop_lan_service");
}

export async function lanSendText(peerId: string, text: string): Promise<boolean> {
  return invoke<boolean>("lan_send_text", { peerId, text });
}

export async function lanSendFiles(peerId: string, paths: string[]): Promise<boolean> {
  // On Android, resolve content:// URIs to real files before sending
  const resolved = isMobile() ? await resolveContentUris(paths) : paths;
  const result = await invoke<boolean>("lan_send_files", { peerId, paths: resolved });
  // Clean up temp files after send
  if (isMobile()) invoke("cleanup_send_cache").catch(() => {});
  return result;
}

export async function setDefaultOutFolder(folder: string): Promise<void> {
  return invoke("set_default_out_folder", { folder });
}

export async function setPeerOutFolder(peerId: string, folder: string): Promise<void> {
  return invoke("set_peer_out_folder", { peerId, folder });
}

export async function setDeviceAlias(alias: string): Promise<void> {
  return invoke("set_device_alias", { alias });
}

export async function getDeviceIdentity(): Promise<DeviceIdentity> {
  return invoke<DeviceIdentity>("get_device_identity");
}

// ── File operations ──

export async function getFileInfo(path: string): Promise<FileInfo | null> {
  try {
    return await invoke<FileInfo>("get_file_info", { path });
  } catch {
    return null;
  }
}

export async function showInExplorer(path: string): Promise<boolean> {
  return invoke<boolean>("show_in_explorer", { path });
}

export async function openFile(path: string): Promise<void> {
  if (isMobile()) {
    // Use Kotlin plugin with FileProvider for proper Android file sharing
    return invoke("plugin:file-helper|openFile", { path });
  }
  return invoke("open_file", { path });
}

export function isMobile(): boolean {
  return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

export async function getThumbnail(path: string): Promise<string | null> {
  return invoke<string | null>("get_thumbnail", { path, maxPx: 120 });
}

export async function getFullImage(path: string, maxPx: number = 800): Promise<string | null> {
  return invoke<string | null>("get_thumbnail", { path, maxPx });
}

export async function getLocalIp(): Promise<string> {
  return invoke<string>("get_local_ip");
}

export async function pickFiles(): Promise<string[] | null> {
  const result = await open({ multiple: true, directory: false });
  if (!result) return null;
  if (Array.isArray(result)) return result;
  return [result];
}

export async function pickFolder(): Promise<string[] | null> {
  const result = await open({ multiple: false, directory: true });
  if (!result) return null;
  if (Array.isArray(result)) return result;
  return [result];
}

export async function pickSaveFolder(): Promise<string | null> {
  const result = await open({ multiple: false, directory: true });
  if (!result) return null;
  if (Array.isArray(result)) return result[0];
  return result;
}

export async function copyToClipboard(text: string): Promise<void> {
  await writeText(text);
}

export async function saveClipboardImage(base64Data: string, mime: string): Promise<string> {
  return invoke<string>("save_clipboard_image", { base64Data, mime });
}

export async function getClipboardFiles(): Promise<string[]> {
  return invoke<string[]>("get_clipboard_files");
}

export interface FilePreview {
  name: string;
  size: string;
  extension: string;
  content: string | null;
  line_count: number;
  truncated: boolean;
}

export async function readFilePreview(path: string, maxLines?: number): Promise<FilePreview> {
  return invoke<FilePreview>("read_file_preview", { path, maxLines });
}

export async function getExplorerSelection(): Promise<string[]> {
  return invoke<string[]>("get_explorer_selection");
}

export async function setMica(enabled: boolean, opacity?: number): Promise<void> {
  return invoke("set_mica", { enabled, opacity: opacity ?? 70 });
}

// ── Event listeners ──
export type UnlistenFn = () => void;

export async function onLanLog(cb: (level: string, text: string) => void): Promise<UnlistenFn> {
  return listen<{ level: string; text: string }>("lan_log", (e) => cb(e.payload.level, e.payload.text));
}

export async function onLanPeerDiscovered(cb: (peer: DiscoveredPeer) => void): Promise<UnlistenFn> {
  return listen<DiscoveredPeer>("lan_peer_discovered", (e) => cb(e.payload));
}

export async function onLanPeerLost(cb: (peerId: string) => void): Promise<UnlistenFn> {
  return listen<{ id: string }>("lan_peer_lost", (e) => cb(e.payload.id));
}

export async function onLanTextReceived(cb: (peerId: string, text: string) => void): Promise<UnlistenFn> {
  return listen<{ peer_id: string; text: string }>("lan_text_received", (e) => cb(e.payload.peer_id, e.payload.text));
}

export async function onLanFilesReceived(
  cb: (peerId: string, files: string[], details: Array<{ name: string; path: string; size: number }>) => void
): Promise<UnlistenFn> {
  return listen<{ peer_id: string; files: string[]; file_details: Array<{ name: string; path: string; size: number }> }>(
    "lan_files_received",
    (e) => cb(e.payload.peer_id, e.payload.files, e.payload.file_details)
  );
}

export interface TransferProgress {
  direction: "send" | "receive";
  phase: "start" | "transferring" | "done";
  total_bytes?: number;
  total_files?: number;
  sent_bytes?: number;
  sent_files?: number;
  received_bytes?: number;
  received_files?: number;
  current_file?: string;
}

export async function onTransferProgress(cb: (progress: TransferProgress) => void): Promise<UnlistenFn> {
  return listen<TransferProgress>("lan_transfer_progress", (e) => cb(e.payload));
}

export async function onDragDrop(
  onDrop: (paths: string[]) => void,
  onOver?: () => void,
  onLeave?: () => void,
): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "over") {
      onOver?.();
    } else if (event.payload.type === "drop") {
      onDrop(event.payload.paths);
    } else if (event.payload.type === "leave") {
      onLeave?.();
    }
  });
}

// Window controls
export async function windowMinimize() { await getCurrentWindow().minimize(); }
export async function windowToggleMaximize() { await getCurrentWindow().toggleMaximize(); }
export async function windowClose() { await getCurrentWindow().hide(); }
export async function windowStartDrag() { await getCurrentWindow().startDragging(); }
export async function windowShow() {
  const win = getCurrentWindow();
  await win.show();
  await win.unminimize();
  await win.setFocus();
}

// ── Android content URI resolution ──
// On Android, file picker returns content:// URIs that Rust can't read directly.
// We use the file-helper plugin to get real filenames, FS plugin to read data,
// and a Rust command to save to a temp file.
async function resolveContentUris(paths: string[]): Promise<string[]> {
  const resolved: string[] = [];
  for (const p of paths) {
    if (p.startsWith("content://")) {
      try {
        const { readFile } = await import("@tauri-apps/plugin-fs");
        // Get real filename via Android ContentResolver (Kotlin plugin)
        let name = `file_${Date.now()}`;
        try {
          const info = await invoke<{ name: string; mimeType: string }>("plugin:file-helper|getFileName", { uri: p });
          if (info.name) name = info.name;
        } catch {
          // Fallback: extract type hint from URI
          const decoded = decodeURIComponent(p);
          if (decoded.includes("image")) name = `IMG_${Date.now()}.jpg`;
          else if (decoded.includes("video")) name = `VID_${Date.now()}.mp4`;
        }
        // Read file data via FS plugin (handles content:// on Android)
        const data = await readFile(p);
        // Save to temp via Rust
        const realPath = await invoke<string>("save_temp_for_send", { name, data: Array.from(data) });
        resolved.push(realPath);
      } catch (e) {
        console.error("Failed to resolve content URI:", p, e);
        resolved.push(p);
      }
    } else {
      resolved.push(p);
    }
  }
  return resolved;
}

// Global shortcuts (desktop only — lazy import to avoid errors on mobile)
export async function registerShortcut(shortcut: string, cb: () => void): Promise<void> {
  if (isMobile()) return;
  const { register, unregister, isRegistered } = await import("@tauri-apps/plugin-global-shortcut");
  const already = await isRegistered(shortcut);
  if (already) await unregister(shortcut);
  await register(shortcut, (event) => {
    if (event.state === "Pressed") cb();
  });
}

export async function unregisterShortcut(shortcut: string): Promise<void> {
  if (isMobile()) return;
  const { unregister, isRegistered } = await import("@tauri-apps/plugin-global-shortcut");
  const already = await isRegistered(shortcut);
  if (already) await unregister(shortcut);
}
