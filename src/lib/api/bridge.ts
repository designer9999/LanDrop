import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  fileNameFromPath,
  imageMimeFromName,
  isImage,
  videoMimeFromName,
} from "$lib/utils/file-utils";
import { VideoLruCache, VIDEO_CACHE_BUDGET_BYTES } from "./video-cache";

export interface StatusResponse {
  ok: boolean;
  app_version: string;
  local_ip: string;
}

export interface FileInfo {
  name: string;
  /** Raw byte count — format with fileSizeStr at render time. */
  size_bytes: number;
  type: string;
  count?: number;
}

export interface PreparedSendPath {
  originalPath: string;
  sendPath: string;
  historyPath: string;
}

export interface DeviceIdentity {
  id: string;
  alias: string;
  device_type: string;
}

export interface PlatformInfo {
  os: string;
  is_wayland: boolean;
  supports_global_hotkeys: boolean;
  supports_window_state_persistence: boolean;
}

export async function getPlatformInfo(): Promise<PlatformInfo> {
  return invoke<PlatformInfo>("get_platform_info");
}

export interface ReceiveFolderSettings {
  default_out_folder: string;
  peer_folders: Record<string, string>;
  sort_by_date: boolean;
}

export interface DiscoveredPeer {
  id: string;
  alias: string;
  device_type: string;
  ip: string;
  port: number;
  network?: "lan" | "tailscale";
  lan_ip?: string | null;
  tailscale_ip?: string | null;
  /** Protocol capabilities ("ack", "chat2"); empty for LanDrop 1.8 peers. */
  caps?: string[];
}

/** delivered: the peer confirmed it; sent: written, but the peer cannot confirm (1.8 peers). */
export interface SendOutcome {
  status: "delivered" | "sent" | "unsupported";
}

export type ChatEventPayload =
  | { kind: "edit"; id: string; text: string }
  | { kind: "delete"; id: string }
  | { kind: "react"; id: string; emoji: string | null };

export interface TailscaleStatus {
  state: "available" | "unavailable" | "stopped";
  message: string;
}

export async function onTailscaleStatus(
  cb: (status: TailscaleStatus) => void,
): Promise<() => void> {
  return listen<TailscaleStatus>("tailscale_status", (event) => cb(event.payload));
}

export async function getStatus(): Promise<StatusResponse> {
  return invoke<StatusResponse>("get_status");
}

// ── LAN Service (zero-config mDNS) ──

export async function startLanService(): Promise<void> {
  return invoke("start_lan_service");
}

export async function refreshLanDiscovery(): Promise<void> {
  return invoke("refresh_lan_discovery");
}

export interface SendOptions {
  peerIp?: string;
  messageId?: string;
  replyTo?: string;
}

export async function lanSendText(
  peerId: string,
  text: string,
  options: SendOptions = {},
): Promise<SendOutcome> {
  return invoke<SendOutcome>("lan_send_text", {
    peerId,
    text,
    peerIp: options.peerIp,
    messageId: options.messageId,
    replyTo: options.replyTo,
  });
}

export async function lanSendFiles(
  peerId: string,
  paths: string[],
  options: SendOptions = {},
  onPrepared?: (files: PreparedSendPath[]) => void,
): Promise<SendOutcome> {
  // On Android, resolve content:// URIs to real files before sending
  const mobile = isMobile();
  const prepared = mobile
    ? await prepareSendPaths(paths)
    : paths.map((path) => ({ originalPath: path, sendPath: path, historyPath: path }));
  onPrepared?.(prepared);

  try {
    return await invoke<SendOutcome>("lan_send_files", {
      peerId,
      paths: prepared.map((file) => file.sendPath),
      peerIp: options.peerIp,
      messageId: options.messageId,
      replyTo: options.replyTo,
    });
  } finally {
    // Always remove temporary Android send copies, including after failed sends.
    if (mobile) await invoke("cleanup_send_cache").catch(() => {});
  }
}

export async function lanSendChatEvent(
  peerId: string,
  event: ChatEventPayload,
  peerIp?: string,
): Promise<SendOutcome> {
  return invoke<SendOutcome>("lan_send_chat_event", { peerId, event, peerIp });
}

/** Cancel an in-flight file send; the pending lanSendFiles call rejects with "cancelled". */
export async function lanCancelTransfer(messageId: string): Promise<boolean> {
  return invoke<boolean>("lan_cancel_transfer", { messageId });
}

export async function setDefaultOutFolder(folder: string): Promise<void> {
  return invoke("set_default_out_folder", { folder });
}

export async function setPeerOutFolder(peerId: string, folder: string): Promise<void> {
  return invoke("set_peer_out_folder", { peerId, folder });
}

export async function getReceiveFolderSettings(): Promise<ReceiveFolderSettings> {
  return invoke<ReceiveFolderSettings>("get_receive_folder_settings");
}

export async function setReceiveSortByDate(enabled: boolean): Promise<void> {
  return invoke("set_receive_sort_by_date", { enabled });
}

export async function setDeviceAlias(alias: string): Promise<string> {
  return invoke<string>("set_device_alias", { alias });
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

export async function openUrl(url: string): Promise<void> {
  return invoke("open_url", { url });
}

/** Open an http(s) link from a chat message in the default browser. */
export async function openChatLink(url: string): Promise<void> {
  return invoke("open_chat_link", { url });
}

export async function downloadFile(path: string): Promise<string> {
  if (isMobile()) {
    // Android owns scoped-storage export. Never materialize the entire file in
    // JS or ask for broad storage access just to save a received attachment.
    const result = await invoke<{ savedPath: string }>("plugin:file-helper|saveToDownloads", {
      path,
    });
    if (!result.savedPath) throw new Error("Android did not return a saved destination");
    return result.savedPath;
  }
  // Legacy non-mobile caller path; Android always uses native export above.
  const { readFile, writeFile, mkdir, exists } = await import("@tauri-apps/plugin-fs");
  const name = fileNameFromPath(path, "file");

  // Copy to /storage/emulated/0/Pictures/LanDrop/ for images, Downloads/ for others
  const targetDir = isImage(name)
    ? "/storage/emulated/0/Pictures/LanDrop"
    : "/storage/emulated/0/Download";

  try {
    const dirExists = await exists(targetDir);
    if (!dirExists) await mkdir(targetDir, { recursive: true });
  } catch {
    /* dir might already exist */
  }

  const targetPath = `${targetDir}/${name}`;
  const data = await readFile(path);
  await writeFile(targetPath, data);
  // Tell Android to scan the file so it appears in Gallery / Files app
  try {
    await invoke("plugin:file-helper|saveToDownloads", { path: targetPath });
  } catch {
    /* scan is best-effort */
  }
  return targetPath;
}

export async function deleteHistoryFiles(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  try {
    await invoke("delete_history_files", { paths });
  } catch {
    // Best-effort cleanup. Real Downloads/Pictures files are never targeted.
  }
}

export async function getContentFileName(
  uri: string,
): Promise<{ name: string; mimeType: string } | null> {
  try {
    return await invoke<{ name: string; mimeType: string }>("plugin:file-helper|getFileName", {
      uri,
    });
  } catch {
    return null;
  }
}

export function isMobile(): boolean {
  return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
}

// ── Media (asset protocol) ──
// Filesystem media is served straight from disk by the asset protocol once
// Rust has added that exact file to the scope; video gets HTTP range requests
// instead of being copied whole into a Blob. Android content:// URIs, which
// the asset protocol cannot read, keep the byte-copy path below.

const allowedMedia = new Map<string, Promise<boolean>>();
let pendingAllow: { paths: string[]; resolve: (allowed: Set<string>) => void } | null = null;

/** Batch scope requests made in the same tick into one IPC call. */
function requestMediaScope(path: string): Promise<boolean> {
  const cached = allowedMedia.get(path);
  if (cached) return cached;
  if (!pendingAllow) {
    let resolve!: (allowed: Set<string>) => void;
    const batch = new Promise<Set<string>>((r) => (resolve = r));
    pendingAllow = { paths: [], resolve };
    const current = pendingAllow;
    queueMicrotask(() => {
      pendingAllow = null;
      invoke<string[]>("allow_media_paths", { paths: current.paths })
        .then((allowed) => current.resolve(new Set(allowed)))
        .catch(() => current.resolve(new Set()));
    });
    batchPromise = batch;
  }
  pendingAllow.paths.push(path);
  const result = batchPromise!.then((allowed) => allowed.has(path));
  allowedMedia.set(path, result);
  result.then((ok) => {
    if (!ok) allowedMedia.delete(path);
  });
  return result;
}
let batchPromise: Promise<Set<string>> | null = null;

/** Full-resolution URL for an image or video on disk (null if not displayable). */
export async function getMediaUrl(path: string): Promise<string | null> {
  if (!path) return null;
  if (path.startsWith("content://")) return mobileImageSrc(path);
  return (await requestMediaScope(path)) ? convertFileSrc(path) : null;
}

const thumbnailUrls = new Map<string, Promise<string | null>>();

/** Small cached JPEG for lists and bubbles; generated once by Rust, reused after. */
export function getThumbnail(path: string, maxPx = 320): Promise<string | null> {
  if (path.startsWith("content://")) return mobileImageSrc(path);
  const key = `${maxPx}|${path}`;
  const cached = thumbnailUrls.get(key);
  if (cached) return cached;
  const load = invoke<string | null>("get_thumbnail", { path, maxPx })
    .then((thumbPath) => (thumbPath ? convertFileSrc(thumbPath) : null))
    .catch(() => null);
  thumbnailUrls.set(key, load);
  load.then((url) => {
    if (!url) thumbnailUrls.delete(key);
  });
  return load;
}

/** Forget cached thumbnail URLs for a path (after the file was deleted or replaced). */
export function forgetMedia(path: string) {
  allowedMedia.delete(path);
  for (const key of thumbnailUrls.keys()) {
    if (key.endsWith(`|${path}`)) thumbnailUrls.delete(key);
  }
}

/** Byte-budgeted, refcounted store of video blob URLs (each is the whole file). */
const videoCache = new VideoLruCache(VIDEO_CACHE_BUDGET_BYTES, (url) => {
  try {
    URL.revokeObjectURL(url);
  } catch {
    /* ignore */
  }
});
const pendingVideoLoads = new Map<string, Promise<string | null>>();

function toBlobPart(data: Uint8Array): ArrayBuffer {
  const buffer = new ArrayBuffer(data.byteLength);
  new Uint8Array(buffer).set(data);
  return buffer;
}

/**
 * Get a playable video source URL. Filesystem videos stream through the asset
 * protocol; only Android content:// URIs are copied into a refcounted Blob —
 * pass every returned URL back to revokeBlobUrl() when the consumer unmounts.
 */
export async function getVideoSrc(path: string): Promise<string | null> {
  if (!path.startsWith("content://")) return getMediaUrl(path);

  const cached = videoCache.acquire(path);
  if (cached) return cached;

  const pending = pendingVideoLoads.get(path);
  if (pending) {
    // Piggyback on the in-flight read, then take our own reference.
    return pending.then(() => videoCache.acquire(path));
  }

  const load = (async () => {
    try {
      const data = await readFileBytes(path);
      const blob = new Blob([toBlobPart(data)], { type: videoMimeFromName(path) });
      const url = URL.createObjectURL(blob);
      videoCache.insert(path, url, blob.size);
      return url;
    } catch {
      return null;
    } finally {
      pendingVideoLoads.delete(path);
    }
  })();
  pendingVideoLoads.set(path, load);
  return load;
}

async function readFileBytes(path: string): Promise<Uint8Array> {
  // Desktop has no fs-plugin grants (see capabilities/default.json) — the
  // scoped plugin read could never cover arbitrary receive folders anyway, so
  // desktop uses the Rust command outright. Android keeps the plugin path
  // because it is the only reader that understands content:// URIs.
  if (isMobile()) {
    try {
      const { readFile } = await import("@tauri-apps/plugin-fs");
      return await readFile(path);
    } catch {
      /* fall through to the Rust command */
    }
  }
  const data = await invoke<ArrayBuffer | number[]>("read_file_bytes", { path });
  return data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data);
}

async function mobileImageSrc(path: string): Promise<string | null> {
  try {
    const { readFile } = await import("@tauri-apps/plugin-fs");
    const data = await readFile(path);
    const blob = new Blob([toBlobPart(data)], { type: imageMimeFromName(path) });
    return URL.createObjectURL(blob);
  } catch {
    return null;
  }
}

/**
 * Release a blob URL obtained from this module. Video URLs drop one cache
 * reference (revoked for real once unreferenced and over the byte budget);
 * any other blob URL is revoked immediately. Safe on non-blob URLs (no-op).
 */
export function revokeBlobUrl(url: string): void {
  if (!url) return;
  if (videoCache.release(url)) return;
  if (url.startsWith("blob:")) {
    try {
      URL.revokeObjectURL(url);
    } catch {
      /* ignore */
    }
  }
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

/** Save pasted image/video bytes to a private cache file and return its path. */
export async function saveClipboardImage(data: Blob | Uint8Array, mime: string): Promise<string> {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(await data.arrayBuffer());
  return invoke<string>("save_clipboard_image", bytes, { headers: { "x-mime": mime } });
}

/** Put an image file on the system clipboard as a bitmap. */
export async function copyImageToClipboard(path: string): Promise<void> {
  const { writeImage } = await import("@tauri-apps/plugin-clipboard-manager");
  const url = await getMediaUrl(path);
  if (!url) throw new Error("Image is not available");
  // The clipboard plugin decodes PNG only; let the webview decode JPEG/WebP/GIF.
  const bitmap = await createImageBitmap(await (await fetch(url)).blob());
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0);
  bitmap.close();
  const png = await canvas.convertToBlob({ type: "image/png" });
  await writeImage(new Uint8Array(await png.arrayBuffer()));
}

export async function getClipboardFiles(): Promise<string[]> {
  return invoke<string[]>("get_clipboard_files");
}

export interface FilePreview {
  name: string;
  /** Raw byte count — format with fileSizeStr at render time. */
  size_bytes: number;
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

export async function setMica(enabled: boolean): Promise<void> {
  return invoke("set_mica", { enabled });
}

// ── Event listeners ──
export type UnlistenFn = () => void;

export async function onLanLog(cb: (level: string, text: string) => void): Promise<UnlistenFn> {
  return listen<{ level: string; text: string }>("lan_log", (e) =>
    cb(e.payload.level, e.payload.text),
  );
}

export async function onLanPeerDiscovered(cb: (peer: DiscoveredPeer) => void): Promise<UnlistenFn> {
  return listen<DiscoveredPeer>("lan_peer_discovered", (e) => cb(e.payload));
}

export async function onLanPeerLost(cb: (peerId: string) => void): Promise<UnlistenFn> {
  return listen<{ id: string }>("lan_peer_lost", (e) => cb(e.payload.id));
}

export interface TextReceivedEvent {
  peer_id: string;
  text: string;
  id?: string | null;
  reply_to?: string | null;
  sent_at?: number | null;
}

export async function onLanTextReceived(
  cb: (event: TextReceivedEvent) => void,
): Promise<UnlistenFn> {
  return listen<TextReceivedEvent>("lan_text_received", (e) => cb(e.payload));
}

export interface FilesReceivedEvent {
  peer_id: string;
  files: string[];
  file_details: Array<{ name: string; path: string; size: number }>;
  id?: string | null;
  reply_to?: string | null;
  /** True when the batch failed after these files were already saved. */
  partial?: boolean;
  expected_files?: number;
}

export async function onLanFilesReceived(
  cb: (event: FilesReceivedEvent) => void,
): Promise<UnlistenFn> {
  return listen<FilesReceivedEvent>("lan_files_received", (e) => cb(e.payload));
}

export type ChatEventReceived = ChatEventPayload & { peer_id: string };

export async function onLanChatEvent(cb: (event: ChatEventReceived) => void): Promise<UnlistenFn> {
  return listen<{
    peer_id: string;
    kind: ChatEventPayload["kind"];
    id: string;
    text?: string | null;
    emoji?: string | null;
  }>("lan_chat_event", (e) => {
    const { peer_id, kind, id } = e.payload;
    if (kind === "edit") cb({ peer_id, kind, id, text: e.payload.text ?? "" });
    else if (kind === "delete") cb({ peer_id, kind, id });
    else if (kind === "react") cb({ peer_id, kind, id, emoji: e.payload.emoji ?? null });
  });
}

export interface TransferProgress {
  direction: "send" | "receive";
  phase: "start" | "transferring" | "done" | "error";
  total_bytes?: number;
  total_files?: number;
  sent_bytes?: number;
  sent_files?: number;
  received_bytes?: number;
  received_files?: number;
  current_file?: string;
  /** Message this transfer belongs to (LanDrop 2 peers and all local sends). */
  message_id?: string | null;
  peer_id?: string | null;
}

export async function onTransferProgress(
  cb: (progress: TransferProgress) => void,
): Promise<UnlistenFn> {
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
export async function windowMinimize() {
  await getCurrentWindow().minimize();
}
export async function windowToggleMaximize() {
  await getCurrentWindow().toggleMaximize();
}
export async function windowClose() {
  await getCurrentWindow().hide();
}
export async function windowStartDrag() {
  await getCurrentWindow().startDragging();
}
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
async function prepareSendPaths(paths: string[]): Promise<PreparedSendPath[]> {
  const resolved: PreparedSendPath[] = [];
  for (const p of paths) {
    if (p.startsWith("content://")) {
      try {
        const { readFile } = await import("@tauri-apps/plugin-fs");
        // Get real filename via Android ContentResolver (Kotlin plugin)
        let name = `file_${Date.now()}`;
        try {
          const info = await invoke<{ name: string; mimeType: string }>(
            "plugin:file-helper|getFileName",
            { uri: p },
          );
          if (info.name) name = info.name;
        } catch {
          // Fallback: extract type hint from URI
          const decoded = decodeURIComponent(p);
          if (decoded.includes("image")) name = `IMG_${Date.now()}.jpg`;
          else if (decoded.includes("video")) name = `VID_${Date.now()}.mp4`;
        }
        // Read file data via FS plugin (handles content:// on Android)
        const data = await readFile(p);
        // Save one copy for transfer and one durable copy for chat history.
        // Raw request bodies: no JSON number arrays for large media.
        const headers = { "x-file-name": encodeURIComponent(name) };
        const realPath = await invoke<string>("save_temp_for_send", data, { headers });
        const historyPath = await invoke<string>("save_history_file", data, { headers });
        resolved.push({ originalPath: p, sendPath: realPath, historyPath });
      } catch (e) {
        console.error("Failed to resolve content URI:", p, e);
        resolved.push({ originalPath: p, sendPath: p, historyPath: p });
      }
    } else {
      resolved.push({ originalPath: p, sendPath: p, historyPath: p });
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
