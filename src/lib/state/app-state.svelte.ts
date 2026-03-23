import type { FileInfo } from "$lib/api/bridge";

// ── Device (auto-discovered via mDNS) ──

export interface DiscoveredDevice {
  id: string;          // UUID from remote device (persistent)
  alias: string;       // display name from mDNS
  deviceType: string;  // "desktop" or "mobile"
  ip: string;          // current IP address
  online: boolean;     // currently discovered on LAN
  color: number;       // auto-assigned avatar color
  outFolder?: string;  // user-configured per-device output folder
}

export interface LogEntry {
  level: "info" | "warn" | "error" | "success";
  text: string;
  time: string;
}

export interface ActivityEntry {
  id: string;
  peerId: string;
  direction: "sent" | "received";
  type: "files" | "text";
  items: string[];
  timestamp: string;
  success: boolean;
  outFolder?: string;
}

export interface MessageAttachment {
  name: string;
  path: string;
  size?: string;
  type: "image" | "file" | "folder";
  fileCount?: number;
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

const DEVICES_KEY = "landrop-devices";
const ACTIVE_DEVICE_KEY = "landrop-active-device";
const MESSAGES_KEY = "landrop-messages";
const ACTIVITY_KEY = "landrop-activity";
const SETTINGS_KEY = "landrop-settings";
const RECEIVE_KEY = "landrop-receive-options";
const HOTKEYS_KEY = "landrop-hotkeys";

export interface HotkeySettings {
  quickSend: string;
  enabled: boolean;
}

const DEFAULT_HOTKEYS: HotkeySettings = { quickSend: "F3", enabled: false };

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? { ...fallback, ...JSON.parse(raw) } : fallback;
  } catch {
    return fallback;
  }
}

function loadArray<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function loadString(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function saveJson(key: string, data: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(data));
  } catch {}
}

function saveString(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {}
}

export interface ReceiveOptions {
  outFolder?: string;
  overwrite?: boolean;
}

class AppState {
  activeView = $state<"transfer" | "settings">("transfer");

  // Devices (auto-discovered + persisted)
  devices = $state<DiscoveredDevice[]>(loadArray<DiscoveredDevice>(DEVICES_KEY));
  activeDeviceId = $state<string | null>(loadString(ACTIVE_DEVICE_KEY));

  // Files
  files = $state<SelectedFile[]>([]);
  sendTextContent = $state("");

  // Transfer state
  transferActive = $state(false);

  // Network
  localIp = $state<string>("...");

  // Logs, activity, messages
  logs = $state<LogEntry[]>([]);
  activity = $state<ActivityEntry[]>(loadArray<ActivityEntry>(ACTIVITY_KEY));
  messages = $state<MessageEntry[]>(loadArray<MessageEntry>(MESSAGES_KEY));

  // Message search & filter
  messageSearch = $state("");
  messageViewAll = $state(false);

  // Settings
  notificationsEnabled = $state<boolean>(loadJson<{ n: boolean }>(SETTINGS_KEY, { n: true }).n);
  receiveOptions = $state<ReceiveOptions>(loadJson(RECEIVE_KEY, {}));
  hotkeys = $state<HotkeySettings>(loadJson(HOTKEYS_KEY, DEFAULT_HOTKEYS));

  // Derived
  get activeDevice(): DiscoveredDevice | null {
    return this.devices.find((d) => d.id === this.activeDeviceId) ?? null;
  }

  get activeDeviceOnline(): boolean {
    return this.activeDevice?.online ?? false;
  }

  get effectiveOutFolder(): string {
    const device = this.activeDevice;
    return device?.outFolder ?? this.receiveOptions.outFolder ?? "";
  }

  get hasFiles(): boolean {
    return this.files.length > 0;
  }

  get filePaths(): string[] {
    return this.files.map((f) => f.path);
  }

  get onlineDevices(): DiscoveredDevice[] {
    return this.devices.filter((d) => d.online);
  }

  // ── Device management (auto-discovered) ──

  /** Called when mDNS discovers or updates a device */
  upsertDevice(peer: { id: string; alias: string; device_type: string; ip: string }) {
    const existing = this.devices.find((d) => d.id === peer.id);
    if (existing) {
      // Update existing — preserve user settings (color, outFolder)
      this.devices = this.devices.map((d) =>
        d.id === peer.id
          ? { ...d, alias: peer.alias, deviceType: peer.device_type, ip: peer.ip, online: true }
          : d
      );
    } else {
      // New device — auto-assign color
      const color = this.devices.length % PEER_COLORS.length;
      this.devices = [
        ...this.devices,
        {
          id: peer.id,
          alias: peer.alias,
          deviceType: peer.device_type,
          ip: peer.ip,
          online: true,
          color,
        },
      ];
    }
    this._saveDevices();

    // Auto-select if no active device
    if (!this.activeDeviceId) {
      this.setActiveDevice(peer.id);
    }
  }

  /** Called when mDNS reports a device left */
  markDeviceOffline(id: string) {
    this.devices = this.devices.map((d) =>
      d.id === id ? { ...d, online: false } : d
    );
    this._saveDevices();
  }

  setActiveDevice(id: string | null) {
    this.activeDeviceId = id;
    saveString(ACTIVE_DEVICE_KEY, id);
  }

  updateDeviceSettings(id: string, updates: Partial<Pick<DiscoveredDevice, "color" | "outFolder">>) {
    this.devices = this.devices.map((d) =>
      d.id === id ? { ...d, ...updates } : d
    );
    this._saveDevices();
  }

  removeDevice(id: string) {
    this.devices = this.devices.filter((d) => d.id !== id);
    if (this.activeDeviceId === id) {
      this.activeDeviceId = this.devices[0]?.id ?? null;
      saveString(ACTIVE_DEVICE_KEY, this.activeDeviceId);
    }
    this._saveDevices();
  }

  private _saveDevices() {
    saveJson(DEVICES_KEY, this.devices);
  }

  // ── Files ──

  addFile(path: string, info: FileInfo | null) {
    if (this.files.some((f) => f.path === path)) return;
    this.files = [...this.files, { path, info }];
  }

  removeFile(path: string) {
    this.files = this.files.filter((f) => f.path !== path);
  }

  clearFiles() {
    this.files = [];
  }

  // ── Logs ──

  addLog(level: LogEntry["level"], text: string) {
    const time = new Date().toLocaleTimeString("en-GB", { hour12: false });
    this.logs = [...this.logs, { level, text, time }];
    if (this.logs.length > 500) this.logs = this.logs.slice(-500);
  }

  clearLogs() {
    this.logs = [];
  }

  // ── Activity ──

  addActivity(entry: Omit<ActivityEntry, "id" | "timestamp">) {
    this.activity = [
      ...this.activity,
      { ...entry, id: crypto.randomUUID(), timestamp: new Date().toISOString() },
    ];
    if (this.activity.length > 200) this.activity = this.activity.slice(-200);
    this._saveActivity();
  }

  clearActivity() {
    this.activity = [];
    this._saveActivity();
  }

  private _saveActivity() {
    saveJson(ACTIVITY_KEY, this.activity);
  }

  // ── Messages ──

  addMessage(entry: Omit<MessageEntry, "id" | "timestamp">) {
    this.messages = [
      ...this.messages,
      { ...entry, id: crypto.randomUUID(), timestamp: new Date().toISOString() },
    ];
    this._pruneMessages();
    this._saveMessages();
  }

  getPeerMessages(peerId: string): MessageEntry[] {
    return this.messages.filter((m) => m.peerId === peerId);
  }

  searchMessages(query: string, peerId?: string): MessageEntry[] {
    const q = query.toLowerCase();
    return this.messages.filter((m) => {
      if (peerId && m.peerId !== peerId) return false;
      return m.text.toLowerCase().includes(q);
    });
  }

  getStarredMessages(peerId?: string): MessageEntry[] {
    return this.messages.filter((m) => {
      if (!m.starred) return false;
      if (peerId && m.peerId !== peerId) return false;
      return true;
    });
  }

  toggleStar(messageId: string) {
    this.messages = this.messages.map((m) =>
      m.id === messageId ? { ...m, starred: !m.starred } : m
    );
    this._saveMessages();
  }

  clearMessages(peerId: string) {
    this.messages = this.messages.filter((m) => m.peerId !== peerId || m.starred);
    this._saveMessages();
  }

  deleteAllMessages(peerId: string) {
    this.messages = this.messages.filter((m) => m.peerId !== peerId);
    this._saveMessages();
  }

  deleteOldMessages(daysOld: number) {
    const cutoff = new Date(Date.now() - daysOld * 86400000).toISOString();
    this.messages = this.messages.filter((m) => m.starred || m.timestamp >= cutoff);
    this._saveMessages();
  }

  private _pruneMessages() {
    if (this.messages.length <= 500) return;
    const starred = this.messages.filter((m) => m.starred);
    const unstarred = this.messages.filter((m) => !m.starred);
    const keep = Math.max(0, 500 - starred.length);
    this.messages = [...unstarred.slice(-keep), ...starred].sort((a, b) =>
      a.timestamp.localeCompare(b.timestamp)
    );
  }

  private _saveMessages() {
    saveJson(MESSAGES_KEY, this.messages);
  }

  // ── Settings ──

  setNotifications(enabled: boolean) {
    this.notificationsEnabled = enabled;
    saveJson(SETTINGS_KEY, { n: enabled });
  }

  updateReceiveOption<K extends keyof ReceiveOptions>(key: K, value: ReceiveOptions[K]) {
    this.receiveOptions = { ...this.receiveOptions, [key]: value };
    saveJson(RECEIVE_KEY, this.receiveOptions);
  }

  updateHotkeys(updates: Partial<HotkeySettings>) {
    this.hotkeys = { ...this.hotkeys, ...updates };
    saveJson(HOTKEYS_KEY, this.hotkeys);
  }
}

let instance: AppState | null = null;

export function getAppState(): AppState {
  if (!instance) instance = new AppState();
  return instance;
}
