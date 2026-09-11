import type { DiscoveredPeer, FileInfo, TailscaleStatus } from "$lib/api/bridge";
import { SvelteDate } from "svelte/reactivity";
import {
  devicesForPersistence,
  loadArray,
  loadJson,
  loadString,
  normalizeHydratedDevices,
  sanitizeAttachments,
  sanitizeMessages,
} from "$lib/persistence/sanitize";
import { DEFAULT_HOTKEYS, PEER_COLORS } from "./model";
import type {
  DiscoveredDevice,
  HotkeySettings,
  LogEntry,
  MessageEntry,
  PersistedAppState,
  ReceiveOptions,
  SelectedFile,
} from "./model";

export { PEER_COLORS } from "./model";
export type {
  DiscoveredDevice,
  HotkeySettings,
  LogEntry,
  MessageAttachment,
  MessageEntry,
  PersistedAppState,
  ReceiveOptions,
  SelectedFile,
} from "./model";

const DEVICES_KEY = "landrop-devices";
const ACTIVE_DEVICE_KEY = "landrop-active-device";
const MESSAGES_KEY = "landrop-messages";
const ACTIVITY_KEY = "landrop-activity";
const SETTINGS_KEY = "landrop-settings";
const RECEIVE_KEY = "landrop-receive-options";
const HOTKEYS_KEY = "landrop-hotkeys";

export class AppState {
  activeView = $state<"transfer" | "settings">("transfer");

  // Devices (auto-discovered + persisted)
  devices = $state<DiscoveredDevice[]>([]);
  activeDeviceId = $state<string | null>(null);

  // Files
  files = $state<SelectedFile[]>([]);
  sendTextContent = $state("");

  // Transfer state
  transferActive = $state(false);
  receivingTransferActive = $state(false);

  // Network
  localIp = $state<string>("...");
  tailscaleStatus = $state<TailscaleStatus | null>(null);
  discoveryError = $state("");

  // Logs and messages
  logs = $state<LogEntry[]>([]);
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

  /** Merge routes by persistent identity, retaining history and device preferences. */
  upsertDevice(peer: Omit<DiscoveredPeer, "port">) {
    const routes = {
      network: peer.network,
      lanIp: peer.lan_ip ?? undefined,
      tailscaleIp: peer.tailscale_ip ?? undefined,
    };
    const existing = this.devices.find((d) => d.id === peer.id);
    if (existing) {
      // Update existing — preserve user settings (color, outFolder)
      this.devices = this.devices.map((d) =>
        d.id === peer.id
          ? {
              ...d,
              ...routes,
              alias: peer.alias,
              deviceType: peer.device_type,
              ip: peer.ip,
              online: true,
            }
          : d,
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
          ...routes,
        },
      ];
    }
    // Auto-select if no active device
    if (
      !this.activeDeviceId &&
      !this.sendTextContent &&
      !this.hasFiles &&
      !this.transferActive &&
      !this.messageViewAll
    ) {
      this.setActiveDevice(peer.id);
    }
  }

  /** Called when mDNS reports a device left */
  markDeviceOffline(id: string) {
    this.devices = this.devices.map((d) => (d.id === id ? { ...d, online: false } : d));
    if (this.activeDeviceId === id) this.activeDeviceId = null;
  }

  markAllDevicesOffline() {
    this.devices = this.devices.map((device) => ({ ...device, online: false }));
    this.activeDeviceId = null;
  }

  setActiveDevice(id: string | null) {
    this.activeDeviceId = this.onlineDevices.some((device) => device.id === id) ? id : null;
    this.messageViewAll = false;
    this.messageSearch = "";
  }

  updateDeviceSettings(
    id: string,
    updates: Partial<Pick<DiscoveredDevice, "color" | "avatarIcon" | "outFolder">>,
  ) {
    this.devices = this.devices.map((d) => (d.id === id ? { ...d, ...updates } : d));
  }

  removeDevice(id: string) {
    this.devices = this.devices.filter((d) => d.id !== id);
    if (this.activeDeviceId === id) {
      this.activeDeviceId = null;
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

  removeSentFiles(files: readonly SelectedFile[]) {
    // Match queue entries so removing and re-adding the same path creates a new draft item.
    // This local membership set is discarded after filtering, never rendered.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const sentFiles = new Set(files);
    this.files = this.files.filter((file) => !sentFiles.has(file));
  }

  clearFiles() {
    this.files = [];
  }

  // ── Logs ──

  addLog(level: LogEntry["level"], text: string) {
    const time = new SvelteDate().toLocaleTimeString("en-GB", { hour12: false });
    this.logs = [...this.logs, { level, text, time }];
    if (this.logs.length > 500) this.logs = this.logs.slice(-500);
  }

  clearLogs() {
    this.logs = [];
  }

  // ── Messages ──

  addMessage(entry: Omit<MessageEntry, "id" | "timestamp">) {
    const attachments = sanitizeAttachments(entry.attachments);
    this.messages = [
      ...this.messages,
      { ...entry, attachments, id: crypto.randomUUID(), timestamp: new SvelteDate().toISOString() },
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
      m.id === messageId ? { ...m, starred: !m.starred } : m,
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
  }

  clearMessages(peerId: string) {
    this.messages = this.messages.filter((m) => m.peerId !== peerId || m.starred);
  }

  deleteAllMessages(peerId: string) {
    this.messages = this.messages.filter((m) => m.peerId !== peerId);
  }

  deleteOldMessages(peerId: string, daysOld: number): MessageEntry[] {
    const cutoff = new SvelteDate(Date.now() - daysOld * 86400000).toISOString();
    const deletedMessages = this.messages.filter(
      (m) => m.peerId === peerId && !m.starred && m.timestamp < cutoff,
    );
    this.messages = this.messages.filter(
      (m) => m.peerId !== peerId || m.starred || m.timestamp >= cutoff,
    );
    return deletedMessages;
  }

  private _pruneMessages() {
    if (this.messages.length <= 500) return;
    const starred = this.messages.filter((m) => m.starred);
    const unstarred = this.messages.filter((m) => !m.starred);
    const keep = Math.max(0, 500 - starred.length);
    const recentUnstarred = keep > 0 ? unstarred.slice(-keep) : [];
    this.messages = [...recentUnstarred, ...starred].sort((a, b) =>
      a.timestamp.localeCompare(b.timestamp),
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
    this.devices = devices;
    // A saved conversation does not establish current availability.
    this.activeDeviceId = null;
    this.messageViewAll = false;
    this.messageSearch = "";
    this.messages = messages;
    this.notificationsEnabled = snapshot.notificationsEnabled ?? true;
    this.popOnReceive = snapshot.popOnReceive ?? false;
    // outFolder/sortByDate are owned by the Rust backend; drop any stale
    // copies carried in older persisted snapshots.
    const {
      outFolder: _outFolder,
      sortByDate: _sortByDate,
      ...receiveOptions
    } = snapshot.receiveOptions ?? {};
    this.receiveOptions = receiveOptions;
    this.hotkeys = { ...DEFAULT_HOTKEYS, ...(snapshot.hotkeys ?? {}) };
  }

  exportPersistedState(): PersistedAppState {
    return {
      version: 1,
      devices: devicesForPersistence(this.devices),
      activeDeviceId: null,
      messages: sanitizeMessages(this.messages),
      notificationsEnabled: this.notificationsEnabled,
      popOnReceive: this.popOnReceive,
      // Receive routing (outFolder/sortByDate) lives in Rust's
      // receive_folders.json — never persisted on the frontend side.
      receiveOptions: {},
      hotkeys: this.hotkeys,
    };
  }

  loadLegacyPersistedState(): PersistedAppState | null {
    const devices = loadArray<DiscoveredDevice>(DEVICES_KEY);
    const activeDeviceId = loadString(ACTIVE_DEVICE_KEY);
    const messages = loadArray<MessageEntry>(MESSAGES_KEY);
    const notifications = loadJson<{ n: boolean }>(SETTINGS_KEY, { n: true });
    const popOnReceive = loadJson<{ pop: boolean }>(SETTINGS_KEY, { pop: false });
    const receiveOptions = loadJson<ReceiveOptions>(RECEIVE_KEY, {});
    const hotkeys = loadJson<HotkeySettings>(HOTKEYS_KEY, DEFAULT_HOTKEYS);

    const hasLegacyState =
      devices.length > 0 ||
      messages.length > 0 ||
      activeDeviceId !== null ||
      loadString(SETTINGS_KEY) !== null ||
      loadString(RECEIVE_KEY) !== null ||
      loadString(HOTKEYS_KEY) !== null;

    if (!hasLegacyState) return null;

    return {
      version: 1,
      devices,
      activeDeviceId,
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
