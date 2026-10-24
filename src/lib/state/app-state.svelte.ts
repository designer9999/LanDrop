import type { DiscoveredPeer, FileInfo, TailscaleStatus } from "$lib/api/bridge";
import { untrack } from "svelte";
import { SvelteDate, SvelteMap } from "svelte/reactivity";
import {
  applyChatEvent,
  applyIncomingFiles,
  applyIncomingText,
  beginIncomingTransfer,
  deleteOwnMessageForEveryone,
  editOwnMessage,
  failIncomingTransfer,
  removeMessage,
  setMessageStatus,
  setMyReaction,
  unreadCount,
} from "$lib/chat/message-reducers";
import type { ChatEvent, IncomingFiles, IncomingText } from "$lib/chat/message-reducers";
import {
  devicesForPersistence,
  loadArray,
  loadJson,
  loadString,
  normalizeHydratedDevices,
  sanitizeAttachments,
  sanitizeLastRead,
  sanitizeMessages,
} from "$lib/persistence/sanitize";
import { DEFAULT_HOTKEYS, PEER_COLORS, PERSISTED_STATE_VERSION } from "./model";
import type {
  DiscoveredDevice,
  Draft,
  HotkeySettings,
  LogEntry,
  MessageEntry,
  MessageProgress,
  MessageStatus,
  PersistedAppState,
  ReceiveOptions,
  SelectedFile,
} from "./model";

/** History cap; starred messages are always kept. */
const MAX_MESSAGES = 2000;

export { PEER_COLORS } from "./model";
export type {
  DiscoveredDevice,
  Draft,
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

/** Draft key for attachments queued before any conversation is open (quick-send hotkey). */
const UNASSIGNED = "";

function emptyDraft(): Draft {
  return { text: "", files: [] };
}

function draftIsEmpty(draft: Draft | undefined): boolean {
  return !draft || (!draft.text.trim() && draft.files.length === 0 && !draft.replyTo);
}

export interface Conversation {
  device: DiscoveredDevice;
  last?: MessageEntry;
  unread: number;
}

export class AppState {
  activeView = $state<"transfer" | "settings">("transfer");

  // Devices (auto-discovered + persisted)
  devices = $state<DiscoveredDevice[]>([]);
  activeDeviceId = $state<string | null>(null);

  // Composer drafts, one per conversation
  drafts = $state<Record<string, Draft>>({});

  // Transfer state
  /** Sends currently in flight (text and files run concurrently). */
  activeSends = $state(0);
  receivingTransferActive = $state(false);
  /** Live per-message attachment progress, keyed by message ID. */
  progress = new SvelteMap<string, MessageProgress>();

  // Network
  localIp = $state<string>("...");
  tailscaleStatus = $state<TailscaleStatus | null>(null);
  discoveryError = $state("");

  // Logs and messages
  logs = $state<LogEntry[]>([]);
  messages = $state<MessageEntry[]>([]);
  lastReadAt = $state<Record<string, string>>({});
  /** Last-read time captured when the open conversation was entered (unread divider). */
  unreadAfter = $state<string | undefined>(undefined);

  // Message search & filter
  messageSearch = $state("");
  starredOnly = $state(false);

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

  get transferActive(): boolean {
    return this.activeSends > 0;
  }

  get effectiveOutFolder(): string {
    const device = this.activeDevice;
    return device?.outFolder ?? this.receiveOptions.outFolder ?? "";
  }

  get onlineDevices(): DiscoveredDevice[] {
    return this.devices.filter((d) => d.online);
  }

  /** Every device that is online or has history, most recent activity first. */
  get conversations(): Conversation[] {
    // Local lookup table built per read, never stored or rendered.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const lastByPeer = new Map<string, MessageEntry>();
    for (const message of this.messages) lastByPeer.set(message.peerId, message);
    return this.devices
      .filter((device) => device.online || lastByPeer.has(device.id))
      .map((device) => ({
        device,
        last: lastByPeer.get(device.id),
        unread:
          device.id === this.activeDeviceId && this.activeView === "transfer"
            ? 0
            : unreadCount(this.messages, device.id, this.lastReadAt[device.id]),
      }))
      .sort((a, b) => {
        const at = a.last?.timestamp ?? "";
        const bt = b.last?.timestamp ?? "";
        if (at !== bt) return bt.localeCompare(at);
        return Number(b.device.online) - Number(a.device.online);
      });
  }

  get totalUnread(): number {
    return this.conversations.reduce((sum, conversation) => sum + conversation.unread, 0);
  }

  // ── Drafts (the composer always edits the open conversation's draft) ──

  private get draftKey(): string {
    return this.activeDeviceId ?? UNASSIGNED;
  }

  get draft(): Draft {
    return this.drafts[this.draftKey] ?? emptyDraft();
  }

  private updateDraft(update: (draft: Draft) => Draft) {
    const key = this.draftKey;
    this.drafts = { ...this.drafts, [key]: update(this.drafts[key] ?? emptyDraft()) };
  }

  get sendTextContent(): string {
    return this.draft.text;
  }

  set sendTextContent(text: string) {
    this.updateDraft((draft) => ({ ...draft, text }));
  }

  get files(): SelectedFile[] {
    return this.draft.files;
  }

  set files(files: SelectedFile[]) {
    this.updateDraft((draft) => ({ ...draft, files }));
  }

  /** Unsent text or attachments in any conversation (blocks app updates). */
  get hasAnyDraft(): boolean {
    return Object.values(this.drafts).some((draft) => !draftIsEmpty(draft));
  }

  get hasFiles(): boolean {
    return this.files.length > 0;
  }

  get filePaths(): string[] {
    return this.files.map((f) => f.path);
  }

  get replyingTo(): MessageEntry | null {
    const id = this.draft.replyTo;
    return id ? (this.messages.find((m) => m.id === id) ?? null) : null;
  }

  get editingMessage(): MessageEntry | null {
    const id = this.draft.editingId;
    return id ? (this.messages.find((m) => m.id === id) ?? null) : null;
  }

  setReplyTo(messageId: string | null) {
    this.updateDraft((draft) => {
      // Leaving edit mode for a reply restores the text typed before editing.
      const { editingId, stash, ...rest } = draft;
      const text = editingId ? (stash ?? "") : draft.text;
      return { ...rest, text, replyTo: messageId ?? undefined };
    });
  }

  /** Enter edit mode: the composer temporarily holds the message's text. */
  startEditing(messageId: string) {
    const message = this.messages.find(
      (m) => m.id === messageId && m.direction === "sent" && !m.deleted,
    );
    if (!message) return;
    this.updateDraft((draft) => ({
      ...draft,
      editingId: messageId,
      replyTo: undefined,
      // Remember the unsent draft so cancelling an edit restores it.
      stash: draft.editingId ? draft.stash : draft.text,
      text: message.text,
    }));
  }

  stopEditing() {
    this.updateDraft((draft) => {
      if (!draft.editingId) return draft;
      const { editingId: _editingId, stash, ...rest } = draft;
      return { ...rest, text: stash ?? "" };
    });
  }

  /** Last own message that can still be edited (for the ↑ shortcut). */
  lastEditableMessage(peerId: string): MessageEntry | null {
    for (let index = this.messages.length - 1; index >= 0; index -= 1) {
      const message = this.messages[index];
      if (message.peerId !== peerId || message.direction !== "sent") continue;
      if (message.deleted || !message.text) return null;
      return message;
    }
    return null;
  }

  // ── Device management (auto-discovered) ──

  /** Merge routes by persistent identity, retaining history and device preferences. */
  upsertDevice(peer: Omit<DiscoveredPeer, "port">) {
    const routes = {
      network: peer.network,
      lanIp: peer.lan_ip ?? undefined,
      tailscaleIp: peer.tailscale_ip ?? undefined,
      caps: Array.isArray(peer.caps) ? peer.caps : undefined,
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
    // Open the first device that appears when nothing else is open — unless
    // files are waiting for the user to choose a recipient.
    if (!this.activeDeviceId && draftIsEmpty(this.drafts[UNASSIGNED])) {
      this.setActiveDevice(peer.id, { adoptUnassigned: false });
    }
  }

  /** Called when mDNS reports a device left. Its conversation stays open, read-only. */
  markDeviceOffline(id: string) {
    this.devices = this.devices.map((d) => (d.id === id ? { ...d, online: false } : d));
  }

  markAllDevicesOffline() {
    this.devices = this.devices.map((device) => ({ ...device, online: false }));
  }

  /** Open a conversation. Offline devices open read-only so their history is reachable. */
  setActiveDevice(id: string | null, { adoptUnassigned = true } = {}) {
    const target = id ? this.devices.find((device) => device.id === id) : null;
    const nextId = target?.id ?? null;
    if (nextId !== this.activeDeviceId && this.activeDeviceId) this.markRead(this.activeDeviceId);
    // Attachments queued before any conversation was open follow the first one opened.
    if (
      adoptUnassigned &&
      nextId &&
      !draftIsEmpty(this.drafts[UNASSIGNED]) &&
      draftIsEmpty(this.drafts[nextId])
    ) {
      const { [UNASSIGNED]: pending, ...rest } = this.drafts;
      this.drafts = { ...rest, [nextId]: pending };
    }
    this.activeDeviceId = nextId;
    this.messageSearch = "";
    this.starredOnly = false;
    if (nextId) {
      this.unreadAfter = this.lastReadAt[nextId];
      this.markRead(nextId);
    } else {
      this.unreadAfter = undefined;
    }
  }

  /** Advance the read marker to the newest message (idempotent, so effects settle). */
  markRead(peerId: string) {
    const latest = this.messages.findLast((m) => m.peerId === peerId)?.timestamp;
    if (!latest) return;
    const current = untrack(() => this.lastReadAt[peerId]);
    if (current && current >= latest) return;
    this.lastReadAt = { ...untrack(() => this.lastReadAt), [peerId]: latest };
  }

  updateDeviceSettings(
    id: string,
    updates: Partial<Pick<DiscoveredDevice, "color" | "avatarIcon" | "outFolder">>,
  ) {
    this.devices = this.devices.map((d) => (d.id === id ? { ...d, ...updates } : d));
  }

  removeDevice(id: string) {
    this.devices = this.devices.filter((d) => d.id !== id);
    const { [id]: _removed, ...drafts } = this.drafts;
    this.drafts = drafts;
    if (this.activeDeviceId === id) {
      this.activeDeviceId = null;
    }
  }

  /** Forget a conversation: its messages, draft and read marker (device stays if online). */
  forgetConversation(id: string) {
    this.messages = this.messages.filter((m) => m.peerId !== id);
    const { [id]: _read, ...lastReadAt } = this.lastReadAt;
    this.lastReadAt = lastReadAt;
    const device = this.devices.find((d) => d.id === id);
    if (device && !device.online) this.removeDevice(id);
  }

  // ── Files ──

  addFile(path: string, info: FileInfo | null) {
    this.addFileTo(this.draftKey, path, info);
  }

  /**
   * Queue a file in one conversation's draft. Async producers (paste, pickers,
   * drops) capture the target first so switching conversations mid-save can't
   * redirect the file to someone else.
   */
  addFileTo(peerId: string | null, path: string, info: FileInfo | null) {
    const key = peerId ?? UNASSIGNED;
    const draft = this.drafts[key] ?? emptyDraft();
    if (draft.files.some((f) => f.path === path)) return;
    this.drafts = { ...this.drafts, [key]: { ...draft, files: [...draft.files, { path, info }] } };
  }

  /** Attachments queued before any conversation was opened (quick-send hotkey). */
  get unassignedFileCount(): number {
    return this.drafts[UNASSIGNED]?.files.length ?? 0;
  }

  removeFile(path: string) {
    this.files = this.files.filter((f) => f.path !== path);
  }

  /** Remove exactly the queue entries that were sent; anything added meanwhile stays. */
  removeSentFiles(files: readonly SelectedFile[], peerId = this.draftKey) {
    // This local membership set is discarded after filtering, never rendered.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const sentFiles = new Set(files);
    const draft = this.drafts[peerId];
    if (!draft) return;
    this.drafts = {
      ...this.drafts,
      [peerId]: { ...draft, files: draft.files.filter((file) => !sentFiles.has(file)) },
    };
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

  private now(): string {
    return new SvelteDate().toISOString();
  }

  addMessage(entry: Omit<MessageEntry, "id" | "timestamp"> & { id?: string }): MessageEntry {
    const attachments = sanitizeAttachments(entry.attachments);
    const message: MessageEntry = {
      ...entry,
      attachments,
      id: entry.id ?? crypto.randomUUID(),
      timestamp: this.now(),
    };
    if (!attachments) delete message.attachments;
    this.messages = [...this.messages, message];
    this._pruneMessages();
    return message;
  }

  getMessage(id: string): MessageEntry | undefined {
    return this.messages.find((m) => m.id === id);
  }

  /** Replace one message in place (no-op if it no longer exists). */
  patchMessage(id: string, patch: Partial<MessageEntry>) {
    const index = this.messages.findIndex((m) => m.id === id);
    if (index < 0) return;
    const next = this.messages.slice();
    next[index] = { ...next[index], ...patch };
    this.messages = next;
  }

  private apply(next: MessageEntry[]) {
    if (next === this.messages) return;
    this.messages = next;
    this._pruneMessages();
  }

  receiveText(incoming: IncomingText) {
    this.apply(applyIncomingText(this.messages, incoming, this.now()));
  }

  receiveFiles(incoming: IncomingFiles) {
    this.apply(applyIncomingFiles(this.messages, incoming, this.now()));
  }

  beginReceiving(peerId: string, id: string) {
    this.apply(beginIncomingTransfer(this.messages, peerId, id, this.now()));
  }

  failReceiving(peerId: string, id: string) {
    this.apply(failIncomingTransfer(this.messages, peerId, id));
    this.progress.delete(id);
  }

  receiveChatEvent(peerId: string, event: ChatEvent) {
    this.apply(applyChatEvent(this.messages, peerId, event, this.now()));
  }

  setStatus(id: string, status: MessageStatus | null, error?: string) {
    this.apply(setMessageStatus(this.messages, id, status, error));
  }

  editOwn(id: string, text: string) {
    this.apply(editOwnMessage(this.messages, id, text, this.now()));
  }

  deleteOwnForEveryone(id: string) {
    this.apply(deleteOwnMessageForEveryone(this.messages, id));
  }

  deleteForMe(id: string) {
    this.apply(removeMessage(this.messages, id));
    this.progress.delete(id);
  }

  react(id: string, emoji: string | null) {
    this.apply(setMyReaction(this.messages, id, emoji));
  }

  getPeerMessages(peerId: string): MessageEntry[] {
    return this.messages.filter((m) => m.peerId === peerId);
  }

  searchMessages(query: string, peerId?: string): MessageEntry[] {
    const q = query.toLowerCase();
    return this.messages.filter((m) => {
      if (peerId && m.peerId !== peerId) return false;
      if (m.text.toLowerCase().includes(q)) return true;
      return (m.attachments ?? []).some((a) => a.name.toLowerCase().includes(q));
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
    if (this.messages.length <= MAX_MESSAGES) return;
    const starred = this.messages.filter((m) => m.starred);
    const unstarred = this.messages.filter((m) => !m.starred);
    const keep = Math.max(0, MAX_MESSAGES - starred.length);
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
    this.messageSearch = "";
    this.messages = messages;
    this.lastReadAt = sanitizeLastRead(snapshot.lastReadAt);
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
      version: PERSISTED_STATE_VERSION,
      devices: devicesForPersistence(this.devices),
      activeDeviceId: null,
      messages: sanitizeMessages(this.messages),
      lastReadAt: this.lastReadAt,
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
