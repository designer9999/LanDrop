/** Menu entries for a message: one source for right-click, long-press and the ⋮ button. */
import {
  copyImageToClipboard,
  copyToClipboard,
  downloadFile,
  isMobile,
  openFile,
  showInExplorer,
} from "$lib/api/bridge";
import type { AppState } from "$lib/state/app-state.svelte";
import { QUICK_REACTIONS, type MessageEntry } from "$lib/state/model";
import { menu, type MenuEntry } from "$lib/ui/menu-state.svelte";
import type { ChatContext } from "./chat-context";

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function messageMenuEntries(
  message: MessageEntry,
  app: AppState,
  chat: ChatContext,
): MenuEntry[] {
  const { controller, notify } = chat;
  const mine = message.direction === "sent";
  const device = app.devices.find((d) => d.id === message.peerId);
  const online = device?.online ?? false;
  const attachments = message.attachments ?? [];
  const single = attachments.length === 1 ? attachments[0] : null;
  const entries: MenuEntry[] = [];

  if (message.deleted) {
    entries.push({
      label: "Remove from this device",
      icon: "delete",
      onselect: () => controller.deleteForMe(message.id),
    });
    return entries;
  }

  if (message.status !== "sending" && message.status !== "receiving") {
    entries.push({
      reactions: QUICK_REACTIONS,
      selected: message.reactions?.mine,
      onreact: (emoji) => void controller.react(message.id, emoji),
    });
  }

  entries.push({
    label: "Reply",
    icon: "reply",
    onselect: () => {
      app.setReplyTo(message.id);
      chat.focusComposer();
    },
  });

  if (message.text) {
    entries.push({
      label: "Copy text",
      icon: "content_copy",
      onselect: async () => {
        const selection = window.getSelection()?.toString();
        await copyToClipboard(selection || message.text);
        notify(selection ? "Copied selection" : "Copied");
      },
    });
  }

  if (single?.type === "image" && !isMobile()) {
    entries.push({
      label: "Copy image",
      icon: "image",
      onselect: async () => {
        try {
          await copyImageToClipboard(single.path);
          notify("Image copied");
        } catch (error) {
          notify(`Couldn't copy image: ${errorText(error)}`);
        }
      },
    });
  }

  if (mine && message.text && message.status !== "sending") {
    entries.push({
      label: "Edit",
      icon: "edit",
      hint: "↑",
      onselect: () => {
        app.startEditing(message.id);
        chat.focusComposer();
      },
    });
  }

  const others = app.onlineDevices.filter((d) => d.id !== message.peerId);
  if (
    (message.text || attachments.length > 0) &&
    others.length > 0 &&
    message.status !== "receiving"
  ) {
    entries.push({
      label: "Forward",
      icon: "forward",
      onselect: () => {
        const { innerWidth, innerHeight } = window;
        menu.open(
          innerWidth / 2 - 110,
          innerHeight / 3,
          others.map((target) => ({
            label: target.alias,
            icon: target.deviceType === "mobile" ? "smartphone" : "computer",
            onselect: () => void controller.forward(message.id, target.id),
          })),
        );
      },
    });
  }

  if (attachments.length > 0 && message.status !== "receiving") {
    if (isMobile()) {
      if (!mine) {
        entries.push({
          label: attachments.length > 1 ? "Save all" : "Save",
          icon: "download",
          onselect: async () => {
            for (const attachment of attachments) {
              try {
                await downloadFile(attachment.path);
              } catch (error) {
                notify(`Save failed: ${errorText(error)}`);
                return;
              }
            }
            notify(attachments.length > 1 ? `Saved ${attachments.length} items` : "Saved");
          },
        });
      }
    } else {
      if (single) {
        entries.push({
          label: "Open",
          icon: "open_in_new",
          onselect: () =>
            void openFile(single.path).catch((error) => notify(`Open failed: ${errorText(error)}`)),
        });
      }
      entries.push({
        label: "Show in folder",
        icon: "folder_open",
        onselect: () =>
          void showInExplorer(attachments[0].path).catch((error) =>
            notify(`Couldn't show the file: ${errorText(error)}`),
          ),
      });
    }
  }

  entries.push({
    label: message.starred ? "Unstar" : "Star",
    icon: message.starred ? "star" : "star_outline",
    onselect: () => app.toggleStar(message.id),
  });

  entries.push("divider");

  if (mine && message.status === "failed") {
    entries.push({
      label: "Retry",
      icon: "refresh",
      disabled: !online,
      onselect: () => void controller.retry(message.id),
    });
  }
  if (mine && message.status === "sending" && message.sendPaths?.length) {
    entries.push({
      label: "Cancel transfer",
      icon: "cancel",
      onselect: () => void controller.cancel(message.id),
    });
  }

  entries.push({
    label: "Delete for me",
    icon: "delete",
    onselect: () => controller.deleteForMe(message.id),
  });
  if (mine && message.status !== "failed") {
    entries.push({
      label: "Delete for everyone",
      icon: "delete_forever",
      danger: true,
      disabled: !online,
      onselect: () => void controller.deleteForEveryone(message.id),
    });
  }
  return entries;
}
