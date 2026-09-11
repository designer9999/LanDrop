/**
 * Pure sanitizers/normalizers for persisted state — no runes, unit-testable.
 * Everything here validates untrusted JSON (Tauri Store snapshots, legacy
 * localStorage) into the typed model.
 */
import { fileNameFromPath } from "$lib/utils/file-utils";
import { PEER_COLORS } from "$lib/state/model";
import type { DiscoveredDevice, MessageAttachment, MessageEntry } from "$lib/state/model";

const MAX_FOLDER_CHILDREN_IN_HISTORY = 100;

export function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? { ...fallback, ...JSON.parse(raw) } : fallback;
  } catch {
    return fallback;
  }
}

export function loadArray<T>(key: string): T[] {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function loadString(key: string): string | null {
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

export function sanitizeAttachment(raw: unknown, depth = 0): MessageAttachment | null {
  if (!isRecord(raw)) return null;

  const path = stringValue(raw.path);
  const name = stringValue(raw.name, fileNameFromPath(path, "item"));
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

export function sanitizeAttachments(raw: unknown): MessageAttachment[] | undefined {
  const values = Array.isArray(raw) ? raw : isRecord(raw) ? [raw] : [];
  const attachments = values
    .map((attachment) => sanitizeAttachment(attachment))
    .filter((attachment): attachment is MessageAttachment => attachment !== null);
  return attachments.length > 0 ? attachments : undefined;
}

export function sanitizeMessage(raw: unknown): MessageEntry | null {
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

export function sanitizeMessages(raw: unknown): MessageEntry[] {
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

export function normalizeHydratedDevices(
  rawDevices: unknown,
  messages: MessageEntry[],
): DiscoveredDevice[] {
  const devices: DiscoveredDevice[] = Array.isArray(rawDevices)
    ? rawDevices
        .filter(isRecord)
        .map((device, index) => ({
          id: stringValue(device.id),
          alias: stringValue(device.alias, stringValue(device.id).slice(0, 8) || "Device"),
          deviceType: stringValue(device.deviceType, stringValue(device.device_type, "desktop")),
          ip: "",
          online: false,
          color: numberValue(device.color) ?? index % PEER_COLORS.length,
          avatarIcon: stringValue(device.avatarIcon) || undefined,
          // outFolder deliberately not restored — Rust owns receive folders
        }))
        .filter((device) => device.id)
    : [];

  const seen = new Set(devices.map((device) => device.id));
  for (const message of messages) {
    if (seen.has(message.peerId)) continue;
    seen.add(message.peerId);
    devices.push(placeholderDevice(message.peerId, devices.length % PEER_COLORS.length));
  }

  return devices;
}

export function devicesForPersistence(devices: DiscoveredDevice[]): DiscoveredDevice[] {
  return devices.map((device) => ({
    ...device,
    online: false,
    ip: "",
    network: undefined,
    lanIp: undefined,
    tailscaleIp: undefined,
    // Receive folders are owned by the Rust backend (receive_folders.json)
    // and hydrated from get_receive_folder_settings at startup.
    outFolder: undefined,
  }));
}
