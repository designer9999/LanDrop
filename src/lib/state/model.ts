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
  caps?: string[]; // protocol capabilities advertised this session ("ack", "chat2")
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

/**
 * sending → sent (peer has no receipts) | delivered (peer acknowledged) | failed.
 * "receiving" marks an incoming batch whose files are still arriving.
 */
export type MessageStatus = "sending" | "sent" | "delivered" | "failed" | "receiving";

export interface MessageReactions {
  mine?: string;
  theirs?: string;
}

export interface MessageEntry {
  /** Shared with the peer when both run LanDrop 2 (sender-generated UUID). */
  id: string;
  peerId: string;
  direction: "sent" | "received";
  text: string;
  timestamp: string;
  starred?: boolean;
  attachments?: MessageAttachment[];
  status?: MessageStatus;
  error?: string;
  replyTo?: string;
  /** Short text of the quoted message, shown when the original isn't available. */
  replyPreview?: string;
  editedAt?: string;
  /** Tombstone: the sender deleted this message for everyone. */
  deleted?: boolean;
  reactions?: MessageReactions;
  /** Original local paths of a sent attachment batch, kept so a failed send can be retried. */
  sendPaths?: string[];
}

/** Live byte progress of a message's attachments (transient, never persisted). */
export interface MessageProgress {
  bytes: number;
  total: number;
  files: number;
  totalFiles: number;
  rateBps?: number;
}

/** Composer state kept separately for every conversation. */
export interface Draft {
  text: string;
  files: SelectedFile[];
  replyTo?: string;
  editingId?: string;
  /** Text typed before entering edit mode, restored when the edit is cancelled. */
  stash?: string;
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
  /** ISO time each conversation was last viewed, for unread badges. */
  lastReadAt?: Record<string, string>;
  notificationsEnabled: boolean;
  popOnReceive: boolean;
  receiveOptions: ReceiveOptions;
  hotkeys: HotkeySettings;
}

export const PERSISTED_STATE_VERSION = 2;

export const QUICK_REACTIONS = ["👍", "❤️", "😂", "😮", "🙏", "🔥"];

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
