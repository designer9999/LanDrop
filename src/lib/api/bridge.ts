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

export async function getStatus(): Promise<StatusResponse> {
  return invoke<StatusResponse>("get_status");
}

export async function startLAN(code: string, outFolder: string): Promise<void> {
  return invoke("start_lan", { code, outFolder });
}

export async function stopLAN(): Promise<void> {
  return invoke("stop_lan");
}

export async function lanSendText(text: string): Promise<boolean> {
  return invoke<boolean>("lan_send_text", { text });
}

export async function lanSendFiles(paths: string[]): Promise<boolean> {
  return invoke<boolean>("lan_send_files", { paths });
}

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

// Event listeners
export type UnlistenFn = () => void;

export async function onLanConnected(cb: (peerIp: string) => void): Promise<UnlistenFn> {
  return listen<{ peer_ip: string }>("lan_connected", (e) => cb(e.payload.peer_ip));
}

export async function onLanDisconnected(cb: () => void): Promise<UnlistenFn> {
  return listen("lan_disconnected", () => cb());
}

export async function onLanTextReceived(cb: (text: string) => void): Promise<UnlistenFn> {
  return listen<{ text: string }>("lan_text_received", (e) => cb(e.payload.text));
}

export async function onLanFilesReceived(
  cb: (files: string[], details: Array<{ name: string; path: string; size: number }>) => void
): Promise<UnlistenFn> {
  return listen<{ files: string[]; file_details: Array<{ name: string; path: string; size: number }> }>(
    "lan_files_received",
    (e) => cb(e.payload.files, e.payload.file_details)
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
export async function windowClose() { await getCurrentWindow().close(); }
export async function windowStartDrag() { await getCurrentWindow().startDragging(); }
