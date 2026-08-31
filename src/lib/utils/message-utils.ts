/** Pure chat-message helpers (no runes — unit-testable). */
import type { MessageEntry } from "$lib/state/model";

export interface BubbleNeighbor {
  direction: "sent" | "received";
  peerId: string;
}

export type BubblePosition = "solo" | "first" | "middle" | "last";

/** Compute chat-bubble grouping for the message at `index`. */
export function getBubblePosition(msgs: readonly BubbleNeighbor[], index: number): BubblePosition {
  const curr = msgs[index];
  const prev = index > 0 ? msgs[index - 1] : null;
  const next = index < msgs.length - 1 ? msgs[index + 1] : null;
  const sameAsPrev = !!prev && prev.direction === curr.direction && prev.peerId === curr.peerId;
  const sameAsNext = !!next && next.direction === curr.direction && next.peerId === curr.peerId;
  if (sameAsPrev && sameAsNext) return "middle";
  if (sameAsPrev) return "last";
  if (sameAsNext) return "first";
  return "solo";
}

/**
 * Newest-first image paths worth keeping thumbnails for, capped at `maxPaths`
 * so a long history cannot drive an unbounded decode/evict loop.
 */
export function newestHistoryThumbnailPaths(
  messages: readonly MessageEntry[],
  maxPaths: number,
): string[] {
  const paths = new Set<string>();
  for (let index = messages.length - 1; index >= 0 && paths.size < maxPaths; index -= 1) {
    for (const attachment of messages[index].attachments ?? []) {
      if (attachment.type === "image" && attachment.path) paths.add(attachment.path);
      if (attachment.type === "folder") {
        for (const child of attachment.children ?? []) {
          if (child.type === "image" && child.path) paths.add(child.path);
          if (paths.size >= maxPaths) break;
        }
      }
      if (paths.size >= maxPaths) break;
    }
  }
  return [...paths];
}

/** Every distinct attachment path (including folder children) in `messages`. */
export function collectAttachmentPaths(messages: readonly MessageEntry[]): string[] {
  const paths = new Set<string>();
  for (const message of messages) {
    for (const attachment of message.attachments ?? []) {
      if (attachment.path) paths.add(attachment.path);
      for (const child of attachment.children ?? []) {
        if (child.path) paths.add(child.path);
      }
    }
  }
  return [...paths];
}
