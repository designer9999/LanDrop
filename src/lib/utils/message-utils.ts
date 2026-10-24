/** Pure chat-message helpers (no runes — unit-testable). */
import type { MessageEntry } from "$lib/state/model";

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
