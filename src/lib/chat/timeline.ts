/** Conversation timeline layout: date separators, unread divider, bubble grouping. */
import type { MessageEntry } from "$lib/state/model";

export type BubblePosition = "solo" | "first" | "middle" | "last";

export type TimelineItem =
  | { kind: "date"; key: string; label: string }
  | { kind: "unread"; key: string; count: number }
  | { kind: "message"; key: string; message: MessageEntry; position: BubblePosition };

/** Messages further apart than this start a new visual group. */
const GROUP_GAP_MS = 5 * 60 * 1000;

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

export function dateLabel(date: Date, now: Date = new Date()): string {
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const day = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const diffDays = Math.round((today.getTime() - day.getTime()) / 86_400_000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays > 1 && diffDays < 7) {
    return date.toLocaleDateString(undefined, { weekday: "long" });
  }
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}

function joins(a: MessageEntry | undefined, b: MessageEntry | undefined): boolean {
  if (!a || !b) return false;
  if (a.direction !== b.direction || a.peerId !== b.peerId) return false;
  const ta = Date.parse(a.timestamp);
  const tb = Date.parse(b.timestamp);
  if (!Number.isFinite(ta) || !Number.isFinite(tb)) return true;
  if (dayKey(new Date(ta)) !== dayKey(new Date(tb))) return false;
  return Math.abs(tb - ta) <= GROUP_GAP_MS;
}

/**
 * Build the rendered timeline. `unreadAfter` is the conversation's last-read
 * time when it was opened; the divider sits before the first newer message.
 */
export function buildTimeline(
  messages: readonly MessageEntry[],
  options: { unreadAfter?: string; now?: Date } = {},
): TimelineItem[] {
  const items: TimelineItem[] = [];
  const now = options.now ?? new Date();
  let lastDay = "";
  let unreadPlaced = !options.unreadAfter;
  const unreadTotal = options.unreadAfter
    ? messages.filter((m) => m.direction === "received" && m.timestamp > options.unreadAfter!)
        .length
    : 0;

  messages.forEach((message, index) => {
    const at = new Date(message.timestamp);
    const valid = Number.isFinite(at.getTime());
    const key = valid ? dayKey(at) : "";
    if (valid && key !== lastDay) {
      items.push({ kind: "date", key: `date-${key}`, label: dateLabel(at, now) });
      lastDay = key;
    }
    if (
      !unreadPlaced &&
      unreadTotal > 0 &&
      message.direction === "received" &&
      message.timestamp > options.unreadAfter!
    ) {
      items.push({ kind: "unread", key: "unread", count: unreadTotal });
      unreadPlaced = true;
    }
    const prev = messages[index - 1];
    const next = messages[index + 1];
    const withPrev = joins(prev, message);
    const withNext = joins(message, next);
    const position: BubblePosition =
      withPrev && withNext ? "middle" : withPrev ? "last" : withNext ? "first" : "solo";
    items.push({ kind: "message", key: message.id, message, position });
  });

  return items;
}

export function formatTime(timestamp: string): string {
  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) return "";
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** Compact relative time for conversation lists: "14:05", "Yesterday", "Mon", "12 Mar". */
export function formatListTime(timestamp: string, now: Date = new Date()): string {
  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) return "";
  const label = dateLabel(date, now);
  if (label === "Today") return formatTime(timestamp);
  if (label === "Yesterday") return label;
  const diffDays = (now.getTime() - date.getTime()) / 86_400_000;
  if (diffDays < 7) return date.toLocaleDateString(undefined, { weekday: "short" });
  return date.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}
