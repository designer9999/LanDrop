/** Shared handlers for the conversation tree, so bubbles don't need prop chains. */
import { getContext, setContext } from "svelte";
import type { ChatController } from "$lib/chat/chat-controller.svelte";

export interface ChatContext {
  controller: ChatController;
  notify: (message: string) => void;
  /** Open the media viewer at an attachment of a message. */
  openMedia: (messageId: string, path: string) => void;
  openFilePreview: (path: string) => void;
  /** Scroll to a message and briefly highlight it (reply quotes). */
  revealMessage: (messageId: string) => void;
  focusComposer: () => void;
}

const KEY = Symbol("chat-context");

export function setChatContext(context: ChatContext) {
  setContext(KEY, context);
}

export function getChatContext(): ChatContext {
  return getContext<ChatContext>(KEY);
}
