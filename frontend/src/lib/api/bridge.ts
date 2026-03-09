/**
 * Typed wrappers for window.pywebview.api calls.
 * In dev mode (no pywebview), falls back to mock data.
 */

declare global {
  interface Window {
    pywebview?: {
      api: Record<string, (...args: any[]) => Promise<any>>;
    };
    onLog?: (level: string, msg: string) => void;
    onEvent?: (event: string, data: any) => void;
  }
}

function api(): Record<string, (...args: any[]) => Promise<any>> | null {
  return window.pywebview?.api ?? null;
}

export interface CrocStatus {
  ok: boolean;
  version?: string;
  app_version?: string;
  error?: string;
  local_ip?: string;
}

export interface UpdateInfo {
  update_available: boolean;
  current_version: string;
  latest_version?: string;
  download_url?: string;
  file_name?: string;
  file_size?: number;
  release_notes?: string;
  error?: string;
}

export interface FileInfo {
  name: string;
  size: string;
  type: string;
  count?: number;
}

export interface SendOptions {
  code?: string;
  curve?: string;
  hash?: string;
  zip?: boolean;
  git?: boolean;
  noCompress?: boolean;
  noLocal?: boolean;
  noMulti?: boolean;
  exclude?: string;
  relay?: string;
  relayPass?: string;
  throttleUpload?: string;
  local?: boolean;
  sourceFolder?: string; // UI-only: quick access folder for file browsing
}

export interface ReceiveOptions {
  outFolder?: string;
  overwrite?: boolean;
  noCompress?: boolean;
  curve?: string;
  relay?: string;
  relayPass?: string;
  local?: boolean;
}

export async function getStatus(): Promise<CrocStatus> {
  return (await api()?.get_status()) ?? { ok: true, version: "v10.3.1 (dev)" };
}

export async function pickFiles(): Promise<string[] | null> {
  return (await api()?.pick_files()) ?? null;
}

export async function pickFolder(): Promise<string[] | null> {
  return (await api()?.pick_folder()) ?? null;
}

export async function pickSaveFolder(): Promise<string | null> {
  return (await api()?.pick_save_folder()) ?? null;
}

export async function getFileInfo(path: string): Promise<FileInfo | null> {
  return (await api()?.get_file_info(path)) ?? null;
}

export async function sendFiles(paths: string[], options?: SendOptions): Promise<void> {
  await api()?.send_files(paths, options ?? {});
}

export async function sendText(text: string, options?: SendOptions): Promise<void> {
  await api()?.send_text(text, options ?? {});
}

export async function receiveFile(code: string, options?: ReceiveOptions): Promise<void> {
  await api()?.receive_file(code, options ?? {});
}

export async function stopTransfer(): Promise<boolean> {
  return (await api()?.stop_transfer()) ?? false;
}

export async function checkUpdate(): Promise<UpdateInfo> {
  return (await api()?.check_update()) ?? { update_available: false, current_version: "dev" };
}

export async function downloadUpdate(url: string): Promise<void> {
  await api()?.download_update(url);
}

export async function launchUpdate(path: string): Promise<void> {
  await api()?.launch_update(path);
}

export async function installCroc(): Promise<void> {
  await api()?.install_croc();
}

export async function openUrl(url: string): Promise<void> {
  await api()?.open_url(url);
}

export async function openFolder(path: string): Promise<boolean> {
  return (await api()?.open_folder(path)) ?? false;
}

export async function copyToClipboard(text: string): Promise<void> {
  await api()?.copy_to_clipboard(text);
}

// ── LAN Direct Transfer ──

export async function startLAN(code: string, outFolder?: string): Promise<void> {
  await api()?.start_lan(code, outFolder ?? "");
}

export async function stopLAN(): Promise<void> {
  await api()?.stop_lan();
}

export async function lanSendText(text: string): Promise<boolean> {
  return (await api()?.lan_send_text(text)) ?? false;
}

export async function lanSendFiles(paths: string[]): Promise<boolean> {
  return (await api()?.lan_send_files(paths)) ?? false;
}

// ── App focus & notifications ──

export async function setFocused(focused: boolean): Promise<void> {
  await api()?.set_focused(focused);
}

export async function setNotifications(enabled: boolean): Promise<void> {
  await api()?.set_notifications(enabled);
}
