<!--
  The open conversation: header, timeline, composer, drag-and-drop, media
  viewer and file preview. Offline conversations stay readable.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";
  import ChatHeader from "./ChatHeader.svelte";
  import MessageList from "./MessageList.svelte";
  import Composer from "./Composer.svelte";
  import MediaViewer from "./MediaViewer.svelte";
  import DropOverlay from "../transfer/DropOverlay.svelte";
  import FilePreviewDialog from "../transfer/FilePreviewDialog.svelte";
  import { setChatContext } from "./chat-context";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { DiscoveredDevice, MessageAttachment, MessageEntry } from "$lib/state/model";
  import type { ChatController } from "$lib/chat/chat-controller.svelte";
  import {
    deleteHistoryFiles,
    getContentFileName,
    getFileInfo,
    isMobile,
    onDragDrop,
    openFile,
    pickFiles,
    pickFolder,
    readFilePreview,
    type FilePreview,
  } from "$lib/api/bridge";
  import { attachmentType } from "$lib/chat/attachments";
  import { collectAttachmentPaths } from "$lib/utils/message-utils";
  import { fileNameFromPath } from "$lib/utils/file-utils";

  interface Props {
    device: DiscoveredDevice;
    controller: ChatController;
    compact?: boolean;
    onsnackbar: (message: string) => void;
    ondevicesettings: (id: string) => void;
  }

  let { device, controller, compact = false, onsnackbar, ondevicesettings }: Props = $props();

  const app = getAppState();
  const mobile = isMobile();

  let list: MessageList | undefined = $state();
  let composer: Composer | undefined = $state();
  let dragOver = $state(false);
  let highlightId = $state<string | null>(null);
  let viewer = $state<{ items: MessageAttachment[]; index: number } | null>(null);
  let preview = $state<{ path: string; data: FilePreview | null; loading: boolean } | null>(null);
  let confirmDelete = $state(false);

  const filtered = $derived(!!app.messageSearch.trim() || app.starredOnly);
  const messages = $derived.by(() => {
    if (app.starredOnly) return app.getStarredMessages(device.id);
    if (app.messageSearch.trim()) return app.searchMessages(app.messageSearch.trim(), device.id);
    return app.getPeerMessages(device.id);
  });

  setChatContext({
    get controller() {
      return controller;
    },
    notify: (message) => onsnackbar(message),
    openMedia,
    openFilePreview,
    revealMessage,
    focusComposer: () => composer?.focus(),
  });

  function mediaOf(attachments: MessageAttachment[] | undefined): MessageAttachment[] {
    return (attachments ?? []).flatMap((a) =>
      a.type === "folder"
        ? (a.children ?? []).filter((c) => c.type === "image" || c.type === "video")
        : a.type === "image" || a.type === "video"
          ? [a]
          : [],
    );
  }

  function openMedia(_messageId: string, path: string) {
    const items = app
      .getPeerMessages(device.id)
      .flatMap((m) => (m.deleted ? [] : mediaOf(m.attachments)));
    const index = items.findIndex((item) => item.path === path);
    viewer =
      index >= 0
        ? { items, index }
        : {
            items: [{ name: fileNameFromPath(path, "media"), path, type: attachmentType(path) }],
            index: 0,
          };
  }

  async function openFilePreview(path: string) {
    const kind = attachmentType(path);
    if (kind === "image" || kind === "video") {
      viewer = { items: [{ name: fileNameFromPath(path, "media"), path, type: kind }], index: 0 };
      return;
    }
    const info = await getFileInfo(path).catch(() => null);
    if (info?.type === "folder") {
      await openFile(path).catch((error) => onsnackbar(`Couldn't open the folder: ${error}`));
      return;
    }
    preview = { path, data: null, loading: true };
    try {
      const data = await readFilePreview(path, 400);
      if (preview?.path === path) preview = { path, data, loading: false };
    } catch {
      if (preview?.path === path) preview = { path, data: null, loading: false };
    }
  }

  function revealMessage(id: string) {
    if (filtered) {
      app.messageSearch = "";
      app.starredOnly = false;
    }
    queueMicrotask(() => {
      if (list?.reveal(id)) {
        highlightId = id;
        setTimeout(() => {
          if (highlightId === id) highlightId = null;
        }, 1600);
      } else {
        onsnackbar("The original message is no longer in this conversation");
      }
    });
  }

  async function addPaths(paths: string[]) {
    // This view belongs to one device; queue there even if the user switches away.
    const target = device.id;
    for (const path of paths) {
      if (mobile && path.startsWith("content://")) {
        // Android: Rust can't read content:// URIs — resolve the name via Kotlin.
        let name = "";
        try {
          name = (await getContentFileName(path))?.name ?? "";
        } catch {
          /* plugin unavailable */
        }
        if (!name) {
          const decoded = decodeURIComponent(path);
          const last = decoded.split("/").pop() ?? "";
          name = last.includes(".")
            ? last
            : decoded.includes("image")
              ? `IMG_${Date.now()}.jpg`
              : decoded.includes("video")
                ? `VID_${Date.now()}.mp4`
                : `file_${Date.now()}`;
        }
        const ext = name.includes(".") ? "." + name.split(".").pop()!.toLowerCase() : "";
        app.addFileTo(target, path, { name, size_bytes: 0, type: ext, count: undefined });
      } else {
        app.addFileTo(target, path, await getFileInfo(path));
      }
    }
    composer?.focus();
  }

  async function handlePickFiles() {
    const result = await pickFiles();
    if (result?.length) await addPaths(result);
  }

  async function handlePickFolder() {
    const result = await pickFolder();
    if (result?.length) await addPaths(result);
  }

  async function openReceiveFolder() {
    const folder = (device.outFolder ?? app.receiveOptions.outFolder)?.trim();
    if (!folder) {
      onsnackbar("Choose a receive folder in Settings first");
      return;
    }
    await openFile(folder).catch(() => onsnackbar("Couldn't open the receive folder"));
  }

  /** Remove app-owned copies only when no remaining message still shows them. */
  function deleteUnreferencedFiles(removed: MessageEntry[]) {
    // Local membership sets, discarded after filtering — never rendered.
    const removedIds = new Set(removed.map((m) => m.id));
    const kept = new Set(collectAttachmentPaths(app.messages.filter((m) => !removedIds.has(m.id))));
    void deleteHistoryFiles(collectAttachmentPaths(removed).filter((path) => !kept.has(path)));
  }

  function clearChat() {
    const removed = app.getPeerMessages(device.id).filter((m) => !m.starred);
    deleteUnreferencedFiles(removed);
    app.clearMessages(device.id);
    onsnackbar(`Cleared chat with ${device.alias}`);
  }

  function deleteConversation() {
    deleteUnreferencedFiles(app.getPeerMessages(device.id));
    const alias = device.alias;
    app.forgetConversation(device.id);
    confirmDelete = false;
    onsnackbar(`Deleted conversation with ${alias}`);
  }

  onMount(() => {
    if (mobile) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    onDragDrop(
      (paths) => {
        dragOver = false;
        if (!app.activeDeviceOnline) {
          onsnackbar(`${device.alias} is offline`);
          return;
        }
        void addPaths(paths);
      },
      () => (dragOver = true),
      () => (dragOver = false),
    )
      .then((fn) => (disposed ? fn() : (unlisten = fn)))
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<section class="chat" aria-label="Conversation with {device.alias}">
  <ChatHeader
    {device}
    {compact}
    onsettings={() => ondevicesettings(device.id)}
    onopenfolder={openReceiveFolder}
    onclear={clearChat}
    ondelete={() => (confirmDelete = true)}
  />

  {#if messages.length > 0}
    <MessageList bind:this={list} {messages} peerAlias={device.alias} {filtered} {highlightId} />
  {:else}
    <div class="empty">
      {#if filtered}
        <Icon name={app.starredOnly ? "star_outline" : "search_off"} size={36} />
        <p>
          {app.starredOnly
            ? "No starred messages yet. Star a message from its menu to keep it here."
            : "No messages match your search."}
        </p>
      {:else}
        <div class="empty-mark"><Icon name="waving_hand" size={30} /></div>
        <p class="empty-title">Say hi to {device.alias}</p>
        <p>Type a message, paste a screenshot, or drop files anywhere in this window.</p>
      {/if}
    </div>
  {/if}

  <Composer
    bind:this={composer}
    peerAlias={device.alias}
    onpickfiles={handlePickFiles}
    onpickfolder={handlePickFolder}
    onaddpaths={addPaths}
  />

  {#if dragOver && app.activeDeviceOnline}
    <DropOverlay />
  {/if}
</section>

{#if viewer}
  <MediaViewer
    items={viewer.items}
    bind:index={viewer.index}
    onclose={() => (viewer = null)}
    {onsnackbar}
  />
{/if}

{#if preview}
  <FilePreviewDialog
    preview={preview.data}
    path={preview.path}
    loading={preview.loading}
    onclose={() => (preview = null)}
    {onsnackbar}
  />
{/if}

<Dialog
  bind:open={confirmDelete}
  headline="Delete this conversation?"
  confirmLabel="Delete"
  dismissLabel="Keep"
  onconfirm={deleteConversation}
  onclose={() => (confirmDelete = false)}
>
  <p>
    All messages with {device.alias} are removed from this device, including starred ones. Received files
    stay in your receive folder.
  </p>
</Dialog>

<style>
  .chat {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 24px;
    text-align: center;
    color: var(--md-sys-color-on-surface-variant);
    font-size: 14px;
  }
  .empty p {
    margin: 0;
    max-width: 320px;
  }
  .empty-mark {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 64px;
    height: 64px;
    margin-bottom: 8px;
    border-radius: 22px;
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
  }
  .empty-title {
    font-size: 18px;
    font-weight: 600;
    font-variation-settings: "wdth" 110;
    color: var(--md-sys-color-on-surface);
  }
</style>
