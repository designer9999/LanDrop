/**
 * Global app state — contacts, transfers, logs, persisted options.
 */

import type { FileInfo, SendOptions, ReceiveOptions } from "$lib/api/bridge";

// ── Types ──

export interface Contact {
  id: string;
  name: string;
  code: string;
  color: number;        // 0-7 index into CONTACT_COLORS
  lastUsedAt: string;
  autoReceive?: boolean;  // auto-accept incoming transfers
  options?: {
    relay?: string;
    curve?: string;
    local?: boolean;
    noCompress?: boolean;
    outFolder?: string;
  };
}

export interface LogEntry {
  level: "info" | "warn" | "error" | "success";
  text: string;
  time: string;
}

export interface ActivityEntry {
  id: string;
  contactId: string;
  direction: "sent" | "received";
  type: "files" | "text";
  items: string[];
  timestamp: string;
  success: boolean;
  outFolder?: string;
}

export interface MessageEntry {
  id: string;
  contactId: string;
  direction: "sent" | "received";
  text: string;
  timestamp: string;
}

export interface SelectedFile {
  path: string;
  info: FileInfo | null;
}

export const CONTACT_COLORS = [
  "#6750A4", // primary purple
  "#00897B", // teal
  "#E65100", // deep orange
  "#1565C0", // blue
  "#AD1457", // pink
  "#558B2F", // green
  "#6D4C41", // brown
  "#546E7A", // blue grey
];

// ── localStorage helpers ──

const SEND_OPTS_KEY = "crude-send-options";
const RECV_OPTS_KEY = "crude-receive-options";
const CONTACTS_KEY = "crude-contacts";
const ACTIVE_CONTACT_KEY = "crude-active-contact";

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
  } catch { /* quota exceeded — ignore */ }
}

function saveString(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch { /* ignore */ }
}

// ── Default options ──

const DEFAULT_SEND_OPTS: SendOptions = {};
const DEFAULT_RECV_OPTS: ReceiveOptions = {};

// ── Migration: create default contact from existing code ──

function migrateExistingCode(): Contact[] {
  const contacts = loadArray<Contact>(CONTACTS_KEY);
  if (contacts.length > 0) return contacts;

  const sendOpts = loadJson<SendOptions>(SEND_OPTS_KEY, {});
  if (sendOpts.code) {
    const defaultContact: Contact = {
      id: crypto.randomUUID(),
      name: "Default",
      code: sendOpts.code,
      color: 0,
      lastUsedAt: new Date().toISOString(),
    };
    return [defaultContact];
  }
  return [];
}

// ── App State ──

class AppState {
  // View
  activeView = $state<"transfer" | "settings">("transfer");

  // Contacts
  contacts = $state<Contact[]>(migrateExistingCode());
  activeContactId = $state<string | null>(loadString(ACTIVE_CONTACT_KEY));

  // Files
  files = $state<SelectedFile[]>([]);

  // Inline text (always enabled — section is collapsible, not toggled)
  sendTextEnabled = $state(true);
  sendTextContent = $state("");

  // Transfer state
  transferActive = $state(false);
  transferMode = $state<"send" | "receive" | null>(null);
  transferCode = $state<string | null>(null);
  crocVersion = $state<string>("...");
  crocOk = $state(true);
  crocInstalling = $state(false);

  // Auto-receive
  autoReceiveActive = $state(false);
  autoReceiveContactId = $state<string | null>(null);

  // Network info
  localIp = $state<string>("...");

  // Logs, activity & messages
  logs = $state<LogEntry[]>([]);
  activity = $state<ActivityEntry[]>([]);
  messages = $state<MessageEntry[]>([]);

  // Persisted global options
  sendOptions = $state<SendOptions>(loadJson(SEND_OPTS_KEY, DEFAULT_SEND_OPTS));
  receiveOptions = $state<ReceiveOptions>(loadJson(RECV_OPTS_KEY, DEFAULT_RECV_OPTS));

  // Transfer queue promise
  private _transferResolve: ((success: boolean) => void) | null = null;

  // ── Derived ──

  get activeContact(): Contact | null {
    return this.contacts.find(c => c.id === this.activeContactId) ?? null;
  }

  get effectiveSendOptions(): SendOptions {
    const contact = this.activeContact;
    if (!contact) return this.sendOptions;
    const opts = { ...this.sendOptions };
    if (contact.options?.relay) opts.relay = contact.options.relay;
    if (contact.options?.curve) opts.curve = contact.options.curve;
    if (contact.options?.local) opts.local = contact.options.local;
    if (contact.options?.noCompress) opts.noCompress = contact.options.noCompress;
    opts.code = contact.code;
    return opts;
  }

  get effectiveReceiveOptions(): ReceiveOptions {
    const contact = this.activeContact;
    const opts = { ...this.receiveOptions };
    if (contact?.options?.outFolder) opts.outFolder = contact.options.outFolder;
    if (contact?.options?.noCompress) opts.noCompress = contact.options.noCompress;
    if (contact?.options?.local) opts.local = contact.options.local;
    if (contact?.options?.relay) opts.relay = contact.options.relay;
    return opts;
  }

  get hasFiles(): boolean {
    return this.files.length > 0;
  }

  get filePaths(): string[] {
    return this.files.map(f => f.path);
  }

  get canSend(): boolean {
    return !this.transferActive && (
      this.hasFiles || !!this.sendTextContent.trim()
    );
  }

  // ── Contact CRUD ──

  addContact(contact: Contact) {
    this.contacts = [...this.contacts, contact];
    this._saveContacts();
  }

  updateContact(id: string, updates: Partial<Contact>) {
    this.contacts = this.contacts.map(c =>
      c.id === id ? { ...c, ...updates } : c
    );
    this._saveContacts();
  }

  removeContact(id: string) {
    this.contacts = this.contacts.filter(c => c.id !== id);
    if (this.activeContactId === id) {
      this.activeContactId = this.contacts[0]?.id ?? null;
      saveString(ACTIVE_CONTACT_KEY, this.activeContactId);
    }
    this._saveContacts();
  }

  setActiveContact(id: string | null) {
    this.activeContactId = id;
    saveString(ACTIVE_CONTACT_KEY, id);
  }

  touchContact(id: string) {
    this.updateContact(id, { lastUsedAt: new Date().toISOString() });
  }

  private _saveContacts() {
    saveJson(CONTACTS_KEY, this.contacts);
  }

  // ── Files ──

  addFile(path: string, info: FileInfo | null) {
    if (this.files.some(f => f.path === path)) return;
    this.files = [...this.files, { path, info }];
  }

  removeFile(path: string) {
    this.files = this.files.filter(f => f.path !== path);
  }

  clearFiles() {
    this.files = [];
  }

  // ── Logs ──

  addLog(level: LogEntry["level"], text: string) {
    const time = new Date().toLocaleTimeString("en-GB", { hour12: false });
    this.logs = [...this.logs, { level, text, time }];
  }

  clearLogs() {
    this.logs = [];
  }

  // ── Activity ──

  addActivity(entry: Omit<ActivityEntry, "id" | "timestamp">) {
    this.activity = [...this.activity, {
      ...entry,
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    }];
  }

  clearActivity() {
    this.activity = [];
  }

  addMessage(entry: Omit<MessageEntry, "id" | "timestamp">) {
    this.messages = [...this.messages, {
      ...entry,
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
    }];
    // Keep max 100 messages
    if (this.messages.length > 100) {
      this.messages = this.messages.slice(-100);
    }
  }

  getContactMessages(contactId: string): MessageEntry[] {
    return this.messages.filter(m => m.contactId === contactId);
  }

  clearMessages(contactId: string) {
    this.messages = this.messages.filter(m => m.contactId !== contactId);
  }

  // ── Transfer orchestration ──

  resetTransfer() {
    this.transferActive = false;
    this.transferMode = null;
    this.transferCode = null;
  }

  resolveTransfer(success: boolean) {
    this.resetTransfer();
    this._transferResolve?.(success);
    this._transferResolve = null;
  }

  waitForTransferDone(): Promise<boolean> {
    return new Promise(resolve => {
      this._transferResolve = resolve;
    });
  }

  // ── Persisted options ──

  saveSendOptions() {
    saveJson(SEND_OPTS_KEY, this.sendOptions);
  }

  saveReceiveOptions() {
    saveJson(RECV_OPTS_KEY, this.receiveOptions);
  }

  updateSendOption<K extends keyof SendOptions>(key: K, value: SendOptions[K]) {
    this.sendOptions = { ...this.sendOptions, [key]: value };
    this.saveSendOptions();
  }

  updateReceiveOption<K extends keyof ReceiveOptions>(key: K, value: ReceiveOptions[K]) {
    this.receiveOptions = { ...this.receiveOptions, [key]: value };
    this.saveReceiveOptions();
  }
}

let instance: AppState | null = null;

export function getAppState(): AppState {
  if (!instance) instance = new AppState();
  return instance;
}
