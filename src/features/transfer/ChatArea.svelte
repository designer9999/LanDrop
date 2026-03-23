<!--
  Chat area orchestrator — toolbar, message list, composer, overlays.
  Sub-components: MessageBubble, Composer, Lightbox, FilePreviewDialog, DropOverlay.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { pickFiles, pickFolder, getFileInfo, copyToClipboard, getThumbnail, getFullImage, readFilePreview, onDragDrop } from "$lib/api/bridge";
  import type { FilePreview } from "$lib/api/bridge";
  import { isImage } from "$lib/utils/file-utils";
  import { onMount } from "svelte";

  import MessageBubble from "./MessageBubble.svelte";
  import Composer from "./Composer.svelte";
  import Lightbox from "./Lightbox.svelte";
  import FilePreviewDialog from "./FilePreviewDialog.svelte";
  import DropOverlay from "./DropOverlay.svelte";

  interface Props {
    peerName?: string;
    onsnackbar?: (msg: string) => void;
    onsend?: () => void;
    onsendtext?: () => void;
  }

  let { peerName, onsnackbar, onsend, onsendtext }: Props = $props();

  const app = getAppState();

  // ── Thumbnail cache (shared across messages + composer) ──
  let thumbCache = $state<Record<string, string>>({});
  const _thumbLoading = new Set<string>();
  const MAX_THUMB_CACHE = 200;

  function loadThumb(path: string) {
    if (path in thumbCache || _thumbLoading.has(path)) return;
    _thumbLoading.add(path);
    getThumbnail(path).then(uri => {
      _thumbLoading.delete(path);
      const keys = Object.keys(thumbCache);
      if (keys.length >= MAX_THUMB_CACHE) {
        const pruned = { ...thumbCache };
        const removeCount = keys.length - MAX_THUMB_CACHE + 1;
        for (const k of keys.slice(0, removeCount)) delete pruned[k];
        thumbCache = pruned;
      }
      thumbCache = { ...thumbCache, [path]: uri ?? "" };
    }).catch(() => {
      _thumbLoading.delete(path);
      thumbCache = { ...thumbCache, [path]: "" };
    });
  }

  // ── Messages state ──
  let messagesEl: HTMLDivElement | undefined = $state();
  let copiedMsgId = $state<string | null>(null);
  let copyTimer: ReturnType<typeof setTimeout>;
  let showStarredOnly = $state(false);
  let showClearMenu = $state(false);
  let searchOpen = $state(false);
  let dragOver = $state(false);

  // ── Lightbox state ──
  let lightboxSrc = $state<string | null>(null);
  let lightboxPath = $state("");
  let lightboxName = $state("");
  let lightboxLoading = $state(false);

  // ── File preview state ──
  let filePreview = $state<FilePreview | null>(null);
  let filePreviewPath = $state("");
  let filePreviewLoading = $state(false);

  // ── Derived messages ──
  const displayMessages = $derived.by(() => {
    const peerId = app.messageViewAll ? undefined : app.activeDevice?.id;
    if (!peerId && !app.messageViewAll) return [];

    if (showStarredOnly) return app.getStarredMessages(peerId);
    if (app.messageSearch) return app.searchMessages(app.messageSearch, peerId);
    if (peerId) return app.getPeerMessages(peerId);
    return [...app.messages];
  });

  const currentPeerMessages = $derived(
    app.messageViewAll
      ? app.messages
      : app.activeDevice
        ? app.getPeerMessages(app.activeDevice.id)
        : []
  );
  const showToolbar = $derived(currentPeerMessages.length > 0 || showStarredOnly || searchOpen);

  // ── Auto-scroll ──
  let wasAtBottom = true;
  $effect(() => {
    if (displayMessages.length && messagesEl && !showStarredOnly && !app.messageSearch) {
      requestAnimationFrame(() => {
        if (messagesEl && wasAtBottom) {
          messagesEl.scrollTop = messagesEl.scrollHeight;
        }
      });
    }
  });

  function handleScroll() {
    if (!messagesEl) return;
    const { scrollTop, scrollHeight, clientHeight } = messagesEl;
    wasAtBottom = scrollTop + clientHeight >= scrollHeight - 60;
  }

  // ── Load thumbnails for visible messages + composer files ──
  $effect(() => {
    for (const msg of displayMessages) {
      if (!msg.attachments) continue;
      for (const att of msg.attachments) {
        if (att.type === "image" && att.path) loadThumb(att.path);
      }
    }
    for (const file of app.files) {
      if (file.info && isImage(file.info.type)) loadThumb(file.path);
    }
  });

  // ── Bubble grouping ──
  function getBubblePosition(index: number): "solo" | "first" | "middle" | "last" {
    const msgs = displayMessages;
    const curr = msgs[index];
    const prev = index > 0 ? msgs[index - 1] : null;
    const next = index < msgs.length - 1 ? msgs[index + 1] : null;
    const sameAsPrev = prev && prev.direction === curr.direction && prev.peerId === curr.peerId;
    const sameAsNext = next && next.direction === curr.direction && next.peerId === curr.peerId;
    if (sameAsPrev && sameAsNext) return "middle";
    if (sameAsPrev) return "last";
    if (sameAsNext) return "first";
    return "solo";
  }

  // ── Handlers ──
  async function handleCopy(msgId: string, text: string) {
    await copyToClipboard(text);
    copiedMsgId = msgId;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { if (copiedMsgId === msgId) copiedMsgId = null; }, 1200);
    onsnackbar?.("Copied to clipboard");
  }

  async function addPaths(paths: string[]) {
    for (const path of paths) {
      const info = await getFileInfo(path);
      app.addFile(path, info);
    }
  }

  async function handlePickFiles() {
    const result = await pickFiles();
    if (result && result.length > 0) await addPaths(result);
  }

  async function handlePickFolder() {
    const result = await pickFolder();
    if (result && result.length > 0) await addPaths(result);
  }

  function handleSend() {
    if (app.hasFiles) {
      onsend?.();
    } else if (app.sendTextContent.trim()) {
      onsendtext?.();
    }
  }

  async function openLightbox(path: string, name: string) {
    lightboxPath = path;
    lightboxName = name;
    lightboxLoading = true;
    lightboxSrc = thumbCache[path] ?? null;
    try {
      const full = await getFullImage(path, 800);
      if (full) lightboxSrc = full;
    } catch {
      // Keep thumbnail as fallback
    } finally {
      lightboxLoading = false;
    }
  }

  function closeLightbox() {
    lightboxSrc = null;
    lightboxPath = "";
    lightboxName = "";
    lightboxLoading = false;
  }

  async function openFilePreview(path: string) {
    filePreviewPath = path;
    filePreviewLoading = true;
    try {
      filePreview = await readFilePreview(path, 200);
    } catch {
      filePreview = null;
    }
    filePreviewLoading = false;
  }

  function closeFilePreview() {
    filePreview = null;
    filePreviewPath = "";
    filePreviewLoading = false;
  }

  // ── Drag-drop via Tauri native API ──
  onMount(() => {
    let unlisten: (() => void) | undefined;
    onDragDrop(
      (paths) => { dragOver = false; addPaths(paths); },
      () => { dragOver = true; },
      () => { dragOver = false; },
    ).then(fn => { unlisten = fn; });

    return () => { unlisten?.(); };
  });
</script>

<div
  class="chat-container"
  class:drag-over={dragOver}
>
  {#if showToolbar}
    <div class="chat-toolbar">
      <button
        class="chip"
        class:chip-active={app.messageViewAll}
        onclick={() => app.messageViewAll = !app.messageViewAll}
      >
        {app.messageViewAll ? "All peers" : "This peer"}
      </button>

      <button
        class="chip"
        class:chip-active={showStarredOnly}
        onclick={() => showStarredOnly = !showStarredOnly}
      >
        <Icon name={showStarredOnly ? "star" : "star_border"} size={11} />
        Saved
      </button>

      <span class="flex-1"></span>

      <button
        class="toolbar-icon"
        class:toolbar-icon-active={searchOpen}
        onclick={() => { searchOpen = !searchOpen; if (!searchOpen) app.messageSearch = ""; }}
        title="Search messages"
      >
        <Icon name="search" size={15} />
      </button>

      <div class="relative">
        <button
          class="toolbar-icon hover:text-error"
          onclick={() => showClearMenu = !showClearMenu}
          title="Clear messages"
        >
          <Icon name="delete_outline" size={15} />
        </button>
        {#if showClearMenu}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="clear-menu" onclick={() => showClearMenu = false}>
            <button onclick={() => { app.activeDevice && app.clearMessages(app.activeDevice.id); }}>
              Clear chat (keep saved)
            </button>
            <button onclick={() => { app.activeDevice && app.deleteAllMessages(app.activeDevice.id); }}>
              Delete all for this peer
            </button>
            <button onclick={() => { app.deleteOldMessages(7); }}>
              Delete older than 7 days
            </button>
            <button onclick={() => { app.deleteOldMessages(30); }}>
              Delete older than 30 days
            </button>
          </div>
        {/if}
      </div>
    </div>

    {#if searchOpen}
      <div class="px-3 pb-2">
        <TextField
          label="Search messages"
          placeholder="Type to search..."
          mono
          bind:value={app.messageSearch}
        />
      </div>
    {/if}
  {/if}

  <div
    bind:this={messagesEl}
    class="chat-messages"
    onscroll={handleScroll}
  >
    {#if displayMessages.length > 0}
      {#each displayMessages as msg, i (msg.id)}
        <MessageBubble
          {msg}
          position={getBubblePosition(i)}
          isCopied={copiedMsgId === msg.id}
          viewAll={app.messageViewAll}
          {thumbCache}
          oncopy={handleCopy}
          onlightbox={openLightbox}
          onfilepreview={openFilePreview}
        />
      {/each}
    {:else if showStarredOnly || app.messageSearch}
      <div class="chat-empty">
        <Icon name={showStarredOnly ? "star_border" : "search_off"} size={32} />
        <span>{showStarredOnly ? "No saved messages" : "No results"}</span>
      </div>
    {:else}
      <div class="chat-empty">
        <div class="chat-empty-icon">
          <Icon name={peerName ? "forum" : "person_search"} size={28} />
        </div>
        <span class="chat-empty-title">{peerName ? `Chat with ${peerName}` : "Select a peer"}</span>
        <span class="chat-empty-hint">
          {peerName ? "Drop files, attach with the clip icon, or type a message" : "Add or select a peer to start transferring"}
        </span>
      </div>
    {/if}
  </div>

  <Composer
    {peerName}
    {thumbCache}
    onpickfiles={handlePickFiles}
    onpickfolder={handlePickFolder}
    onsend={handleSend}
    onlightbox={openLightbox}
    onfilepreview={openFilePreview}
    onaddpaths={addPaths}
  />

  {#if dragOver}
    <DropOverlay />
  {/if}

  {#if lightboxSrc}
    <Lightbox
      src={lightboxSrc}
      name={lightboxName}
      path={lightboxPath}
      loading={lightboxLoading}
      onclose={closeLightbox}
    />
  {/if}

  {#if filePreview || filePreviewLoading}
    <FilePreviewDialog
      preview={filePreview}
      path={filePreviewPath}
      loading={filePreviewLoading}
      onclose={closeFilePreview}
    />
  {/if}
</div>

<style>
  .chat-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .drag-over {
    outline: 2px dashed var(--md-sys-color-primary);
    outline-offset: -2px;
    border-radius: 12px;
  }

  .chat-toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 12px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    padding: 2px 9px;
    border-radius: 99px;
    border: none;
    cursor: pointer;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
    transition: all var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .chip-active {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  .toolbar-icon {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 3px;
    border-radius: 4px;
    color: var(--md-sys-color-on-surface-variant);
    transition: all var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .toolbar-icon-active {
    color: var(--md-sys-color-primary);
  }

  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px 105px;
    display: flex;
    flex-direction: column;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }

  .chat-empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 8px; flex: 1; padding: 40px 20px;
    text-align: center;
    animation: empty-in var(--md-spring-default-spatial-dur) var(--md-spring-default-spatial) both;
  }
  @keyframes empty-in {
    from { opacity: 0; transform: scale(0.95); }
    to   { opacity: 1; transform: scale(1); }
  }
  .chat-empty-icon {
    display: flex; align-items: center; justify-content: center;
    width: 56px; height: 56px; border-radius: 16px;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
    margin-bottom: 4px;
  }
  .chat-empty-title {
    font-size: 14px; font-weight: 500;
    color: var(--md-sys-color-on-surface);
  }
  .chat-empty-hint {
    font-size: 12px; line-height: 16px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.7; max-width: 240px;
  }

  .clear-menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 10;
    background: var(--md-sys-color-surface-container-high);
    border: 1px solid var(--md-sys-color-outline-variant);
    border-radius: 8px;
    padding: 4px 0;
    min-width: 200px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.3);
    animation: menu-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes menu-in {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .clear-menu button {
    display: block; width: 100%; text-align: left;
    padding: 8px 12px; background: transparent; border: none;
    color: var(--md-sys-color-on-surface); font-size: 12px;
    cursor: pointer; transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .clear-menu button:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
</style>
