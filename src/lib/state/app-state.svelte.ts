import type { FileInfo } from "$lib/api/bridge";

// ── Device (auto-discovered via mDNS) ──

export interface DiscoveredDevice {
  id: string;          // UUID from remote device (persistent)
  alias: string;       // display name from mDNS
  deviceType: string;  // "desktop" or "mobile"
  ip: string;          // current IP address
  online: boolean;     // currently discovered on LAN
  color: number;       // auto-assigned avatar color
  avatarIcon?: string; // optional user-selected avatar icon
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
const MAX_FOLDER_CHILDREN_IN_HISTORY = 100;

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isMessageDirection(value: unknown): value is MessageEntry["direction"] {
  return value === "sent" || value === "received";
}

function isAttachmentType(value: unknown): value is MessageAttachment["type"] {
  return value === "image" || value === "video" || value === "file" || value === "folder";
}

function stringValue(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function numberValue(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "";
}

function sanitizeAttachment(raw: unknown, depth = 0): MessageAttachment | null {
  if (!isRecord(raw)) return null;

  const path = stringValue(raw.path);
  const name = stringValue(raw.name, basename(path) || "item");
  let type: MessageAttachment["type"] = isAttachmentType(raw.type) ? raw.type : "file";
  const rawChildren = Array.isArray(raw.children)
    ? raw.children
    : isRecord(raw.children)
      ? [raw.children]
      : [];

  if (rawChildren.length > 0) type = "folder";
  if (!name && !path) return null;

  const attachment: MessageAttachment = {
    name,
    path,
    type,
  };

  const size = stringValue(raw.size);
  if (size) attachment.size = size;

  const fileCount = numberValue(raw.fileCount);
  if (fileCount !== undefined) attachment.fileCount = fileCount;

  if (type === "folder" && depth < 1) {
    const children = rawChildren
      .slice(0, MAX_FOLDER_CHILDREN_IN_HISTORY)
      .map((child) => sanitizeAttachment(child, depth + 1))
      .filter((child): child is MessageAttachment => child !== null);

    if (children.length > 0) attachment.children = children;
    attachment.fileCount = Math.max(fileCount ?? 0, rawChildren.length, children.length);
  }

  return attachment;
}

function sanitizeAttachments(raw: unknown): MessageAttachment[] | undefined {
  const values = Array.isArray(raw)
    ? raw
    : isRecord(raw)
      ? [raw]
      : [];
  const attachments = values
    .map((attachment) => sanitizeAttachment(attachment))
    .filter((attachment): attachment is MessageAttachment => attachment !== null);
  return attachments.length > 0 ? attachments : undefined;
}

function sanitizeMessage(raw: unknown): MessageEntry | null {
  if (!isRecord(raw)) return null;

  const peerId = stringValue(raw.peerId);
  const direction = isMessageDirection(raw.direction) ? raw.direction : null;
  if (!peerId || !direction) return null;

  const id = stringValue(raw.id, crypto.randomUUID());
  const timestamp = stringValue(raw.timestamp, new Date().toISOString());
  const text = stringValue(raw.text);
  const attachments = sanitizeAttachments(raw.attachments);

  const message: MessageEntry = {
    id,
    peerId,
    direction,
    text,
    timestamp,
  };
  if (raw.starred === true) message.starred = true;
  if (attachments) message.attachments = attachments;
  return message;
}

function sanitizeMessages(raw: unknown): MessageEntry[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((message) => sanitizeMessage(message))
    .filter((message): message is MessageEntry => message !== null);
}

function placeholderDevice(peerId: string, color: number): DiscoveredDevice {
  return {
    id: peerId,
    alias: `Device-${peerId.slice(0, 8)}`,
    deviceType: "desktop",
    ip: "",
    online: false,
    color,
  };
}

function normalizeHydratedDevices(
  rawDevices: unknown,
  messages: MessageEntry[],
): DiscoveredDevice[] {
  const devices: DiscoveredDevice[] = Array.isArray(rawDevices) ? rawDevices.filter(isRecord).map((device, index) => ({
    id: stringValue(device.id),
    alias: stringValue(device.alias, stringValue(device.id).slice(0, 8) || "Device"),
    deviceType: stringValue(device.deviceType, stringValue(device.device_type, "desktop")),
    ip: stringValue(device.ip),
    online: false,
    color: numberValue(device.color) ?? index % PEER_COLORS.length,
    avatarIcon: stringValue(device.avatarIcon) || undefined,
    outFolder: stringValue(device.outFolder) || undefined,
  })).filter((device) => device.id) : [];

  const seen = new Set(devices.map((device) => device.id));
  for (const message of messages) {
    if (seen.has(message.peerId)) continue;
    seen.add(message.peerId);
    devices.push(placeholderDevice(message.peerId, devices.length % PEER_COLORS.length));
  }

  return devices;
}

function devicesForPersistence(devices: DiscoveredDevice[]): DiscoveredDevice[] {
  return devices.map((device) => ({
    ...device,
    online: false,
  }));
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
    if (this.activeDeviceId === id) {
      this.activeDeviceId = this.onlineDevices[0]?.id ?? null;
    }
  }

  /** Clear all offline devices (used by refresh button) */
  clearOfflineDevices() {
    this.devices = this.devices.filter((d) => d.online);
    if (this.activeDeviceId && !this.devices.some((device) => device.id === this.activeDeviceId)) {
      this.activeDeviceId = this.onlineDevices[0]?.id ?? null;
    }
  }

  markAllDevicesOffline() {
    this.devices = this.devices.map((device) => ({ ...device, online: false }));
    this.activeDeviceId = null;
  }

  setActiveDevice(id: string | null) {
    this.activeDeviceId = id;
  }

  updateDeviceSettings(id: string, updates: Partial<Pick<DiscoveredDevice, "color" | "avatarIcon" | "outFolder">>) {
    this.devices = this.devices.map((d) =>
      d.id === id ? { ...d, ...updates } : d
    );
  }

  removeDevice(id: string) {
    this.devices = this.devices.filter((d) => d.id !== id);
    if (this.activeDeviceId === id) {
      this.activeDeviceId = this.onlineDevices[0]?.id ?? null;
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
    const attachments = sanitizeAttachments(entry.attachments);
    this.messages = [
      ...this.messages,
      { ...entry, attachments, id: crypto.randomUUID(), timestamp: new Date().toISOString() },
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
    const messages = sanitizeMessages(snapshot.messages);
    const devices = normalizeHydratedDevices(snapshot.devices, messages);
    const activeDeviceId = snapshot.activeDeviceId ?? null;

    this.devices = devices;
    this.activeDeviceId = activeDeviceId && devices.some((device) => device.id === activeDeviceId && device.online)
      ? activeDeviceId
      : null;
    this.activity = Array.isArray(snapshot.activity) ? snapshot.activity : [];
    this.messages = messages;
    this.notificationsEnabled = snapshot.notificationsEnabled ?? true;
    this.popOnReceive = snapshot.popOnReceive ?? false;
    this.receiveOptions = snapshot.receiveOptions ?? {};
    this.hotkeys = { ...DEFAULT_HOTKEYS, ...(snapshot.hotkeys ?? {}) };
  }

  exportPersistedState(): PersistedAppState {
    return {
      version: 1,
      devices: devicesForPersistence(this.devices),
      activeDeviceId: this.activeDevice?.online ? this.activeDeviceId : null,
      activity: this.activity,
      messages: sanitizeMessages(this.messages),
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
      messages: sanitizeMessages(messages),
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
