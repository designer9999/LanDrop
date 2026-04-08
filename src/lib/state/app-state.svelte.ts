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

export interface PersistedAppState {
  version: number;
  devices: DiscoveredDevice[];
  activeDeviceId: string | null;
  activity: ActivityEntry[];
  messages: MessageEntry[];
  notificationsEnabled: boolean;
  popOnReceive: boolean;
  receiveOptions: ReceiveOptions;
  hotkeys: HotkeySettings;
}

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

export interface ReceiveOptions {
  outFolder?: string;
  overwrite?: boolean;
  sortByDate?: boolean;
}

class AppState {
  activeView = $state<"transfer" | "settings">("transfer");

  // Devices (auto-discovered + persisted)
  devices = $state<DiscoveredDevice[]>([]);
  activeDeviceId = $state<string | null>(null);

  // Files
  files = $state<SelectedFile[]>([]);
  sendTextContent = $state("");

  // Transfer state
  transferActive = $state(false);

  // Network
  localIp = $state<string>("...");

  // Logs, activity, messages
  logs = $state<LogEntry[]>([]);
  activity = $state<ActivityEntry[]>([]);
  messages = $state<MessageEntry[]>([]);

  // Message search & filter
  messageSearch = $state("");
  messageViewAll = $state(false);

  // Settings
  notificationsEnabled = $state<boolean>(true);
  popOnReceive = $state<boolean>(false);
  receiveOptions = $state<ReceiveOptions>({});
  hotkeys = $state<HotkeySettings>({ ...DEFAULT_HOTKEYS });

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
  }

  setActiveDevice(id: string | null) {
    this.activeDeviceId = id;
  }

  updateDeviceSettings(id: string, updates: Partial<Pick<DiscoveredDevice, "color" | "outFolder">>) {
    this.devices = this.devices.map((d) =>
      d.id === id ? { ...d, ...updates } : d
    );
  }

  removeDevice(id: string) {
    this.devices = this.devices.filter((d) => d.id !== id);
    if (this.activeDeviceId === id) {
      this.activeDeviceId = this.devices[0]?.id ?? null;
    }
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
  }

  clearActivity() {
    this.activity = [];
  }

  // ── Messages ──

  addMessage(entry: Omit<MessageEntry, "id" | "timestamp">) {
    this.messages = [
      ...this.messages,
      { ...entry, id: crypto.randomUUID(), timestamp: new Date().toISOString() },
    ];
    this._pruneMessages();
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
  }

  updateAttachmentPath(messageId: string, oldPath: string, newPath: string) {
    let changed = false;
    this.messages = this.messages.map((m) => {
      if (m.id !== messageId || !m.attachments?.length) return m;

      const attachments = m.attachments.map((attachment) => {
        const nextChildren = attachment.children?.map((child) => {
          if (child.path !== oldPath) return child;
          changed = true;
          return { ...child, path: newPath };
        });

        if (attachment.path === oldPath) {
          changed = true;
          return { ...attachment, path: newPath, children: nextChildren };
        }

        if (nextChildren) {
          return { ...attachment, children: nextChildren };
        }

        return attachment;
      });

      return changed ? { ...m, attachments } : m;
    });

    if (changed) this.messages = [...this.messages];
  }

  clearMessages(peerId: string) {
    this.messages = this.messages.filter((m) => m.peerId !== peerId || m.starred);
  }

  deleteAllMessages(peerId: string) {
    this.messages = this.messages.filter((m) => m.peerId !== peerId);
  }

  deleteOldMessages(daysOld: number) {
    const cutoff = new Date(Date.now() - daysOld * 86400000).toISOString();
    this.messages = this.messages.filter((m) => m.starred || m.timestamp >= cutoff);
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

  // ── Settings ──

  setNotifications(enabled: boolean) {
    this.notificationsEnabled = enabled;
  }

  setPopOnReceive(enabled: boolean) {
    this.popOnReceive = enabled;
  }

  updateReceiveOption<K extends keyof ReceiveOptions>(key: K, value: ReceiveOptions[K]) {
    this.receiveOptions = { ...this.receiveOptions, [key]: value };
  }

  updateHotkeys(updates: Partial<HotkeySettings>) {
    this.hotkeys = { ...this.hotkeys, ...updates };
  }

  hydratePersistedState(snapshot: PersistedAppState) {
    this.devices = Array.isArray(snapshot.devices) ? snapshot.devices : [];
    this.activeDeviceId = snapshot.activeDeviceId ?? null;
    this.activity = Array.isArray(snapshot.activity) ? snapshot.activity : [];
    this.messages = Array.isArray(snapshot.messages) ? snapshot.messages : [];
    this.notificationsEnabled = snapshot.notificationsEnabled ?? true;
    this.popOnReceive = snapshot.popOnReceive ?? false;
    this.receiveOptions = snapshot.receiveOptions ?? {};
    this.hotkeys = { ...DEFAULT_HOTKEYS, ...(snapshot.hotkeys ?? {}) };
  }

  exportPersistedState(): PersistedAppState {
    return {
      version: 1,
      devices: this.devices,
      activeDeviceId: this.activeDeviceId,
      activity: this.activity,
      messages: this.messages,
      notificationsEnabled: this.notificationsEnabled,
      popOnReceive: this.popOnReceive,
      receiveOptions: this.receiveOptions,
      hotkeys: this.hotkeys,
    };
  }

  loadLegacyPersistedState(): PersistedAppState | null {
    const devices = loadArray<DiscoveredDevice>(DEVICES_KEY);
    const activeDeviceId = loadString(ACTIVE_DEVICE_KEY);
    const activity = loadArray<ActivityEntry>(ACTIVITY_KEY);
    const messages = loadArray<MessageEntry>(MESSAGES_KEY);
    const notifications = loadJson<{ n: boolean }>(SETTINGS_KEY, { n: true });
    const popOnReceive = loadJson<{ pop: boolean }>(SETTINGS_KEY, { pop: false });
    const receiveOptions = loadJson<ReceiveOptions>(RECEIVE_KEY, {});
    const hotkeys = loadJson<HotkeySettings>(HOTKEYS_KEY, DEFAULT_HOTKEYS);

    const hasLegacyState = devices.length > 0
      || activity.length > 0
      || messages.length > 0
      || activeDeviceId !== null
      || loadString(SETTINGS_KEY) !== null
      || loadString(RECEIVE_KEY) !== null
      || loadString(HOTKEYS_KEY) !== null;

    if (!hasLegacyState) return null;

    return {
      version: 1,
      devices,
      activeDeviceId,
      activity,
      messages,
      notificationsEnabled: notifications.n,
      popOnReceive: popOnReceive.pop,
      receiveOptions,
      hotkeys,
    };
  }

  clearLegacyPersistedState() {
    try {
      for (const key of [
        DEVICES_KEY,
        ACTIVE_DEVICE_KEY,
        MESSAGES_KEY,
        ACTIVITY_KEY,
        SETTINGS_KEY,
        RECEIVE_KEY,
        HOTKEYS_KEY,
      ]) {
        localStorage.removeItem(key);
      }
    } catch {}
  }
}

let instance: AppState | null = null;

export function getAppState(): AppState {
  if (!instance) instance = new AppState();
  return instance;
}
