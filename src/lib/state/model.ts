/**
 * Shared application model types and constants.
 * Pure declarations — no runes — so persistence code and tests can import
 * them without compiling Svelte state modules.
 */
import type { FileInfo } from "$lib/api/bridge";

// ── Device (discovered over LAN or Tailscale) ──

export interface DiscoveredDevice {
  id: string; // UUID from remote device (persistent)
  alias: string; // display name from mDNS
  deviceType: string; // "desktop" or "mobile"
  ip: string; // current IP address
  online: boolean; // currently discovered in this session
  network?: "lan" | "tailscale"; // preferred/last successful route
  lanIp?: string;
  tailscaleIp?: string;
  color: number; // auto-assigned avatar color
  avatarIcon?: string; // optional user-selected avatar icon
  outFolder?: string; // per-device output folder (display copy; Rust owns it)
}

export interface LogEntry {
  level: "info" | "warn" | "error" | "success";
  text: string;
  time: string;
}

export interface MessageAttachment {
  name: string;
  path: string;
  size?: string;
  type: "image" | "video" | "file" | "folder";
  fileCount?: number;
  children?: MessageAttachment[];
}

export interface MessageEntry {
  id: string;
  peerId: string;
  direction: "sent" | "received";
  text: string;
  timestamp: string;
  starred?: boolean;
  attachments?: MessageAttachment[];
}

export interface SelectedFile {
  path: string;
  info: FileInfo | null;
}

export interface HotkeySettings {
  quickSend: string;
  enabled: boolean;
}

export const DEFAULT_HOTKEYS: HotkeySettings = { quickSend: "F3", enabled: false };

export interface ReceiveOptions {
  outFolder?: string;
  sortByDate?: boolean;
}

export interface PersistedAppState {
  version: number;
  devices: DiscoveredDevice[];
  activeDeviceId: string | null;
  messages: MessageEntry[];
  notificationsEnabled: boolean;
  popOnReceive: boolean;
  receiveOptions: ReceiveOptions;
  hotkeys: HotkeySettings;
}

export const PEER_COLORS = [
  "#6750A4",
  "#00897B",
  "#E65100",
  "#1565C0",
  "#AD1457",
  "#558B2F",
  "#6D4C41",
  "#546E7A",
];
