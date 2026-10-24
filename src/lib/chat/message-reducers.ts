/**
 * Pure conversation reducers (no runes — unit-testable).
 *
 * Every function returns a new array when something changed and the same array
 * otherwise, so callers can skip reactive writes for no-op events.
 */
import type { MessageAttachment, MessageEntry, MessageStatus } from "$lib/state/model";

export interface IncomingText {
  peerId: string;
  text: string;
  id?: string | null;
  replyTo?: string | null;
  replyPreview?: string | null;
}

const MAX_REPLY_PREVIEW = 200;

function withReply(
  entry: MessageEntry,
  replyTo: string | null | undefined,
  replyPreview: string | null | undefined,
) {
  if (!replyTo) return;
  entry.replyTo = replyTo;
  const preview = replyPreview?.trim();
  if (preview) entry.replyPreview = preview.slice(0, MAX_REPLY_PREVIEW);
}

export interface IncomingFiles {
  peerId: string;
  attachments: MessageAttachment[];
  id?: string | null;
  replyTo?: string | null;
  replyPreview?: string | null;
  partial?: boolean;
}

export type ChatEvent =
  | { kind: "edit"; id: string; text: string }
  | { kind: "delete"; id: string }
  | { kind: "react"; id: string; emoji: string | null };

const newId = () => crypto.randomUUID();

/**
 * A remote ID is only trusted inside its own conversation: a peer must never be
 * able to address (and so overwrite) a message that belongs to someone else.
 */
function findReceived(messages: readonly MessageEntry[], peerId: string, id: string) {
  return messages.findIndex(
    (m) => m.id === id && m.peerId === peerId && m.direction === "received",
  );
}

function idIsFree(messages: readonly MessageEntry[], id: string) {
  return !messages.some((m) => m.id === id);
}

function replaceAt(messages: readonly MessageEntry[], index: number, next: MessageEntry) {
  const copy = messages.slice();
  copy[index] = next;
  return copy;
}

/** Add or merge an incoming text. A batch with the same ID may have arrived first. */
export function applyIncomingText(
  messages: readonly MessageEntry[],
  incoming: IncomingText,
  now: string,
): MessageEntry[] {
  const id = incoming.id ?? null;
  if (id) {
    const index = findReceived(messages, incoming.peerId, id);
    if (index >= 0) {
      const existing = messages[index];
      if (existing.deleted) return messages as MessageEntry[];
      return replaceAt(messages, index, { ...existing, text: incoming.text });
    }
  }
  const entry: MessageEntry = {
    id: id && idIsFree(messages, id) ? id : newId(),
    peerId: incoming.peerId,
    direction: "received",
    text: incoming.text,
    timestamp: now,
  };
  withReply(entry, incoming.replyTo, incoming.replyPreview);
  return [...messages, entry];
}

/**
 * A retried batch reuses its message ID and resends every file; an attachment
 * with the same name replaces the earlier copy instead of appearing twice.
 */
function mergeAttachments(
  existing: readonly MessageAttachment[],
  incoming: readonly MessageAttachment[],
): MessageAttachment[] {
  const names = new Set(incoming.map((attachment) => attachment.name));
  return [...existing.filter((attachment) => !names.has(attachment.name)), ...incoming];
}

/** Add or merge incoming attachments (caption text may already be present). */
export function applyIncomingFiles(
  messages: readonly MessageEntry[],
  incoming: IncomingFiles,
  now: string,
): MessageEntry[] {
  const id = incoming.id ?? null;
  const status: MessageStatus | undefined = incoming.partial ? "failed" : undefined;
  const error = incoming.partial ? "Some files did not arrive" : undefined;
  if (id) {
    const index = findReceived(messages, incoming.peerId, id);
    if (index >= 0) {
      const existing = messages[index];
      const next: MessageEntry = {
        ...existing,
        attachments: mergeAttachments(existing.attachments ?? [], incoming.attachments),
        status,
        error,
      };
      if (!status) {
        delete next.status;
        delete next.error;
      }
      return replaceAt(messages, index, next);
    }
  }
  const entry: MessageEntry = {
    id: id && idIsFree(messages, id) ? id : newId(),
    peerId: incoming.peerId,
    direction: "received",
    text: "",
    timestamp: now,
    attachments: incoming.attachments,
  };
  withReply(entry, incoming.replyTo, incoming.replyPreview);
  if (status) {
    entry.status = status;
    entry.error = error;
  }
  return [...messages, entry];
}

/** Placeholder bubble for an incoming batch whose files are still arriving. */
export function beginIncomingTransfer(
  messages: readonly MessageEntry[],
  peerId: string,
  id: string,
  now: string,
): MessageEntry[] {
  const index = findReceived(messages, peerId, id);
  if (index >= 0) {
    return replaceAt(messages, index, { ...messages[index], status: "receiving" });
  }
  if (!idIsFree(messages, id)) return messages as MessageEntry[];
  return [
    ...messages,
    { id, peerId, direction: "received", text: "", timestamp: now, status: "receiving" },
  ];
}

/** An incoming batch failed: drop an empty placeholder, otherwise flag it. */
export function failIncomingTransfer(
  messages: readonly MessageEntry[],
  peerId: string,
  id: string,
): MessageEntry[] {
  const index = findReceived(messages, peerId, id);
  if (index < 0) return messages as MessageEntry[];
  const existing = messages[index];
  if (existing.status !== "receiving") return messages as MessageEntry[];
  if (!existing.text && !existing.attachments?.length) {
    return messages.filter((_, i) => i !== index);
  }
  return replaceAt(messages, index, {
    ...existing,
    status: "failed",
    error: "The transfer was interrupted",
  });
}

/**
 * Apply a peer's edit/delete/reaction. Edits and deletions only affect messages
 * that peer sent us; reactions may target any message in the conversation.
 */
export function applyChatEvent(
  messages: readonly MessageEntry[],
  peerId: string,
  event: ChatEvent,
  now: string,
): MessageEntry[] {
  if (event.kind === "react") {
    const index = messages.findIndex((m) => m.id === event.id && m.peerId === peerId);
    if (index < 0) return messages as MessageEntry[];
    const existing = messages[index];
    const reactions = { ...existing.reactions };
    if (event.emoji) reactions.theirs = event.emoji.slice(0, 16);
    else delete reactions.theirs;
    const next: MessageEntry = { ...existing, reactions };
    if (!reactions.mine && !reactions.theirs) delete next.reactions;
    return replaceAt(messages, index, next);
  }

  const index = findReceived(messages, peerId, event.id);
  if (index < 0) return messages as MessageEntry[];
  const existing = messages[index];
  if (existing.deleted) return messages as MessageEntry[];
  if (event.kind === "edit") {
    if (existing.text === event.text) return messages as MessageEntry[];
    return replaceAt(messages, index, { ...existing, text: event.text, editedAt: now });
  }
  return replaceAt(messages, index, tombstone(existing));
}

function tombstone(message: MessageEntry): MessageEntry {
  return {
    id: message.id,
    peerId: message.peerId,
    direction: message.direction,
    timestamp: message.timestamp,
    text: "",
    deleted: true,
  };
}

/** Local edit of one of our own sent messages. */
export function editOwnMessage(
  messages: readonly MessageEntry[],
  id: string,
  text: string,
  now: string,
): MessageEntry[] {
  const index = messages.findIndex((m) => m.id === id && m.direction === "sent" && !m.deleted);
  if (index < 0 || messages[index].text === text) return messages as MessageEntry[];
  return replaceAt(messages, index, { ...messages[index], text, editedAt: now });
}

/** "Delete for everyone" on our side: our own message becomes a tombstone. */
export function deleteOwnMessageForEveryone(
  messages: readonly MessageEntry[],
  id: string,
): MessageEntry[] {
  const index = messages.findIndex((m) => m.id === id && m.direction === "sent");
  if (index < 0) return messages as MessageEntry[];
  return replaceAt(messages, index, tombstone(messages[index]));
}

export function removeMessage(messages: readonly MessageEntry[], id: string): MessageEntry[] {
  const next = messages.filter((m) => m.id !== id);
  return next.length === messages.length ? (messages as MessageEntry[]) : next;
}

export function setMyReaction(
  messages: readonly MessageEntry[],
  id: string,
  emoji: string | null,
): MessageEntry[] {
  const index = messages.findIndex((m) => m.id === id);
  if (index < 0) return messages as MessageEntry[];
  const existing = messages[index];
  const reactions = { ...existing.reactions };
  if (emoji) reactions.mine = emoji;
  else delete reactions.mine;
  const next: MessageEntry = { ...existing, reactions };
  if (!reactions.mine && !reactions.theirs) delete next.reactions;
  return replaceAt(messages, index, next);
}

export function setMessageStatus(
  messages: readonly MessageEntry[],
  id: string,
  status: MessageStatus | null,
  error?: string,
): MessageEntry[] {
  const index = messages.findIndex((m) => m.id === id);
  if (index < 0) return messages as MessageEntry[];
  const next: MessageEntry = { ...messages[index] };
  if (status) next.status = status;
  else delete next.status;
  if (error) next.error = error;
  else delete next.error;
  return replaceAt(messages, index, next);
}

/** Count received messages newer than the conversation's last-read time. */
export function unreadCount(
  messages: readonly MessageEntry[],
  peerId: string,
  lastReadAt: string | undefined,
): number {
  let count = 0;
  for (const m of messages) {
    if (m.peerId !== peerId || m.direction !== "received" || m.deleted) continue;
    if (!lastReadAt || m.timestamp > lastReadAt) count += 1;
  }
  return count;
}

/** One-line preview of a message for conversation lists and reply quotes. */
export function messagePreview(message: MessageEntry | undefined): string {
  if (!message) return "";
  if (message.deleted) return "Message deleted";
  const text = message.text.replace(/\s+/g, " ").trim();
  if (text) return text;
  const attachments = message.attachments ?? [];
  if (attachments.length === 0) {
    return message.status === "receiving" ? "Receiving files…" : "";
  }
  if (attachments.length === 1) {
    const [only] = attachments;
    if (only.type === "image") return "Photo";
    if (only.type === "video") return "Video";
    return only.name;
  }
  const images = attachments.filter((a) => a.type === "image").length;
  const videos = attachments.filter((a) => a.type === "video").length;
  if (images === attachments.length) return `${images} photos`;
  if (videos === attachments.length) return `${videos} videos`;
  return `${attachments.length} items`;
}
