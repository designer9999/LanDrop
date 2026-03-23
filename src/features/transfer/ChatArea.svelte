<!--
  Chat interface — messages + bottom composer with inline file attachments.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { MessageAttachment } from "$lib/state/app-state.svelte";
  import { pickFiles, pickFolder, getFileInfo, copyToClipboard, getThumbnail, getFullImage, showInExplorer, onDragDrop, saveClipboardImage, getClipboardFiles, readFilePreview } from "$lib/api/bridge";
  import type { FilePreview } from "$lib/api/bridge";
  import { isImage } from "$lib/utils/file-utils";
  import { onMount } from "svelte";

  let thumbCache = $state<Record<string, string>>({});
  const _thumbLoading = new Set<string>();

  interface Props {
    peerName?: string;
    onsnackbar?: (msg: string) => void;
    onsend?: () => void;
    onsendtext?: () => void;
  }

  let { peerName, onsnackbar, onsend, onsendtext }: Props = $props();

  const app = getAppState();

  let messagesEl: HTMLDivElement | undefined = $state();
  let copiedMsgId = $state<string | null>(null);
  let showStarredOnly = $state(false);
  let showClearMenu = $state(false);
  let searchOpen = $state(false);
  let dragOver = $state(false);
  let composerEl: HTMLTextAreaElement | undefined = $state();

  let fabMenuOpen = $state(false);
  let lightboxSrc = $state<string | null>(null);
  let lightboxPath = $state("");
  let lightboxName = $state("");
  let lightboxLoading = $state(false);

  let filePreview = $state<FilePreview | null>(null);
  let filePreviewPath = $state("");
  let filePreviewLoading = $state(false);

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

  const displayMessages = $derived.by(() => {
    const peerId = app.messageViewAll ? undefined : app.activePeer?.id;
    if (!peerId && !app.messageViewAll) return [];

    let msgs: import("$lib/state/app-state.svelte").MessageEntry[];

    if (showStarredOnly) {
      msgs = app.getStarredMessages(peerId);
    } else if (app.messageSearch) {
      msgs = app.searchMessages(app.messageSearch, peerId);
    } else if (peerId) {
      msgs = app.getPeerMessages(peerId);
    } else {
      msgs = [...app.messages];
    }
    return msgs;
  });

  const currentPeerMessages = $derived(
    app.messageViewAll
      ? app.messages
      : app.activePeer
        ? app.getPeerMessages(app.activePeer.id)
        : []
  );
  const showToolbar = $derived(currentPeerMessages.length > 0 || showStarredOnly || searchOpen);

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

  async function handleCopy(msgId: string, text: string) {
    await copyToClipboard(text);
    copiedMsgId = msgId;
    setTimeout(() => { if (copiedMsgId === msgId) copiedMsgId = null; }, 1200);
    onsnackbar?.("Copied to clipboard");
  }

  function getPeerName(peerId: string): string {
    return app.peers.find(p => p.id === peerId)?.name ?? "Unknown";
  }

  function formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  const MAX_THUMB_CACHE = 200;

  function loadThumb(path: string) {
    if (path in thumbCache || _thumbLoading.has(path)) return;
    _thumbLoading.add(path);
    getThumbnail(path).then(uri => {
      _thumbLoading.delete(path);
      const keys = Object.keys(thumbCache);
      if (keys.length >= MAX_THUMB_CACHE) {
        const pruned = { ...thumbCache };
        for (const k of keys.slice(0, keys.length - MAX_THUMB_CACHE + 1)) delete pruned[k];
        thumbCache = { ...pruned, [path]: uri ?? "" };
      } else {
        thumbCache = { ...thumbCache, [path]: uri ?? "" };
      }
    }).catch(() => {
      _thumbLoading.delete(path);
      thumbCache = { ...thumbCache, [path]: "" };
    });
  }

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

  // Use Tauri's native drag-drop API for proper file path access
  onMount(() => {
    let unlisten: (() => void) | undefined;
    onDragDrop(
      (paths) => { dragOver = false; addPaths(paths); },
      () => { dragOver = true; },
      () => { dragOver = false; },
    ).then(fn => { unlisten = fn; });

    function handleWindowClick() { fabMenuOpen = false; }
    window.addEventListener("click", handleWindowClick);

    return () => {
      unlisten?.();
      window.removeEventListener("click", handleWindowClick);
    };
  });

  function handleSend() {
    if (app.hasFiles) {
      onsend?.();
    } else if (app.sendTextContent.trim()) {
      onsendtext?.();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  async function handlePaste(e: ClipboardEvent) {
    if (!e.clipboardData) return;

    // Check for images first (screenshots, etc.)
    const items = e.clipboardData.items;
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const blob = item.getAsFile();
        if (!blob) continue;
        const reader = new FileReader();
        reader.onload = async () => {
          const dataUrl = reader.result as string;
          const base64 = dataUrl.split(",")[1];
          if (!base64) return;
          const path = await saveClipboardImage(base64, item.type);
          const info = await getFileInfo(path);
          app.addFile(path, info);
        };
        reader.readAsDataURL(blob);
        return;
      }
    }

    // Check for files copied from Explorer (PDF, HTML, etc.)
    try {
      const files = await getClipboardFiles();
      if (files.length > 0) {
        e.preventDefault();
        await addPaths(files);
      }
    } catch {
      // No file paths in clipboard, let normal paste happen
    }
  }

  const canSend = $derived(!app.transferActive && (app.hasFiles || !!app.sendTextContent.trim()));
  const imageFiles = $derived(app.files.filter(f => f.info && isImage(f.info.type)));
  const otherFiles = $derived(app.files.filter(f => !f.info || !isImage(f.info.type)));
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
            <button onclick={() => { app.activePeer && app.clearMessages(app.activePeer.id); }}>
              Clear chat (keep saved)
            </button>
            <button onclick={() => { app.activePeer && app.deleteAllMessages(app.activePeer.id); }}>
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
        {@const pos = getBubblePosition(i)}
        {@const isSent = msg.direction === "sent"}
        {@const hasAttachments = msg.attachments && msg.attachments.length > 0}
        {@const images = msg.attachments?.filter(a => a.type === "image") ?? []}
        {@const files = msg.attachments?.filter(a => a.type === "file") ?? []}
        <div
          class="msg-row"
          class:msg-row-sent={isSent}
          class:msg-row-received={!isSent}
          class:msg-group-break={pos === "solo" || pos === "first"}
        >
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="bubble"
            class:bubble-sent={isSent}
            class:bubble-received={!isSent}
            class:bubble-solo={pos === "solo"}
            class:bubble-first={pos === "first"}
            class:bubble-middle={pos === "middle"}
            class:bubble-last={pos === "last"}
            class:bubble-copied={copiedMsgId === msg.id}
            class:bubble-media={hasAttachments && !msg.text}
            onclick={() => msg.text && handleCopy(msg.id, msg.text)}
            title={msg.text ? "Click to copy" : ""}
          >
            {#if app.messageViewAll}
              <div class="bubble-contact">{getPeerName(msg.peerId)}</div>
            {/if}

            {#if images.length > 0}
              <div class="att-images" class:att-grid={images.length > 1}>
                {#each images as img}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div class="att-img-wrap" onclick={(e) => { e.stopPropagation(); openLightbox(img.path, img.name); }}>
                    {#if thumbCache[img.path] && thumbCache[img.path] !== ""}
                      <img src={thumbCache[img.path]} alt={img.name} class="att-img" />
                    {:else}
                      <div class="att-img-placeholder"><Icon name="image" size={24} /></div>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}

            {#if files.length > 0}
              <div class="att-files">
                {#each files as file}
                  {@const ext = file.name.split('.').pop()?.toUpperCase() ?? 'FILE'}
                  <!-- svelte-ignore a11y_click_events_have_key_events -->
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div class="att-file-card" onclick={(e) => { e.stopPropagation(); openFilePreview(file.path); }} title="Preview file">
                    <span class="att-file-name">{file.name}</span>
                    {#if file.size}<span class="att-file-size">{file.size}</span>{/if}
                    <span class="att-file-badge">{ext}</span>
                  </div>
                {/each}
              </div>
            {/if}

            {#if msg.text}
              <span class="bubble-text">{msg.text}</span>
            {/if}

            <span class="bubble-time-spacer"></span>
            <span class="bubble-meta">
              {#if copiedMsgId === msg.id}
                <span class="bubble-copied-badge"><Icon name="check" size={10} /> Copied</span>
              {:else}
                <button
                  class="star-btn"
                  class:starred={msg.starred}
                  onclick={(e) => { e.stopPropagation(); app.toggleStar(msg.id); }}
                  title={msg.starred ? "Unsave" : "Save"}
                >
                  <Icon name={msg.starred ? "star" : "star_border"} size={11} />
                </button>
                <span class="bubble-time">{formatTime(msg.timestamp)}</span>
              {/if}
            </span>
          </div>
        </div>
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

  <!-- Composer -->
  <div class="composer">
    <!-- FAB menu pills — outside composer-box so overflow:hidden doesn't clip them -->
    {#if fabMenuOpen}
      <div class="fab-menu-items">
        <button class="fab-menu-item" style="animation-delay: 0ms;" onclick={(e) => { e.stopPropagation(); fabMenuOpen = false; handlePickFiles(); }}>
          <Icon name="attach_file" size={18} />
          <span class="text-sm font-medium">Files</span>
        </button>
        <button class="fab-menu-item" style="animation-delay: 40ms;" onclick={(e) => { e.stopPropagation(); fabMenuOpen = false; handlePickFolder(); }}>
          <Icon name="folder" size={18} />
          <span class="text-sm font-medium">Folder</span>
        </button>
      </div>
    {/if}

    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="composer-box" onclick={(e) => { if ((e.target as HTMLElement).closest('button, .composer-img')) return; composerEl?.focus(); }}>
      {#if app.hasFiles}
        <div class="composer-attachments">
          {#each imageFiles as file (file.path)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="composer-img" onclick={() => openLightbox(file.path, file.info?.name ?? "image")}>
              {#if thumbCache[file.path] && thumbCache[file.path] !== ""}
                <img src={thumbCache[file.path]} alt={file.info?.name ?? "image"} class="composer-img-preview" />
              {:else}
                <div class="composer-img-loading">
                  <div class="composer-img-shimmer"></div>
                </div>
              {/if}
              {#if !app.transferActive}
                <button class="composer-img-remove" onclick={(e) => { e.stopPropagation(); app.removeFile(file.path); }}>
                  <Icon name="close" size={14} />
                </button>
              {/if}
            </div>
          {/each}
          {#each otherFiles as file (file.path)}
            {@const name = file.info?.name ?? file.path.split(/[\\/]/).pop() ?? 'file'}
            {@const ext = name.split('.').pop()?.toUpperCase() ?? 'FILE'}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="composer-file-card" onclick={() => openFilePreview(file.path)}>
              <Icon name={file.info?.type === "folder" ? "folder" : "description"} size={18} />
              <span class="composer-file-name">{name}</span>
              {#if file.info?.size}
                <span class="composer-file-size">{file.info.size}</span>
              {/if}
              <span class="att-file-badge">{file.info?.type === "folder" ? "FOLDER" : ext}</span>
              {#if !app.transferActive}
                <button class="composer-file-remove" onclick={(e) => { e.stopPropagation(); app.removeFile(file.path); }}>
                  <Icon name="close" size={14} />
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
      <textarea
        bind:this={composerEl}
        class="composer-textarea"
        placeholder={peerName ? `Message ${peerName}...` : "Type a message..."}
        rows={1}
        bind:value={app.sendTextContent}
        onkeydown={handleKeydown}
        onpaste={handlePaste}
      ></textarea>

      <div class="composer-actions">
        <button
          class="fab-btn"
          style="
            transform: rotate({fabMenuOpen ? '45deg' : '0deg'});
            border-radius: {fabMenuOpen ? '50%' : '12px'};
            background-color: var({fabMenuOpen ? '--color-primary' : '--color-primary-container'});
            color: var({fabMenuOpen ? '--color-on-primary' : '--color-on-primary-container'});
            transition:
              transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
              border-radius var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
              background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
              color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
          onclick={(e) => { e.stopPropagation(); fabMenuOpen = !fabMenuOpen; }}
          title={fabMenuOpen ? "Close" : "Attach"}
        >
          <Icon name="add" size={20} />
        </button>

        <span class="flex-1"></span>

        <button
          class="send-btn"
          class:send-active={canSend}
          disabled={!canSend}
          onclick={handleSend}
          title="Send"
        >
          <Icon name="arrow_upward" size={20} />
        </button>
      </div>
    </div>
  </div>

  {#if dragOver}
    <div class="drag-overlay">
      <Icon name="upload_file" size={40} />
      <span>Drop files here</span>
    </div>
  {/if}

  {#if lightboxSrc}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="lightbox" onclick={closeLightbox}>
      <div class="lightbox-header">
        <span class="lightbox-name">{lightboxName}</span>
        <div class="lightbox-actions">
          <button class="lightbox-btn" onclick={(e) => { e.stopPropagation(); showInExplorer(lightboxPath); }} title="Open in folder">
            <Icon name="folder_open" size={18} />
          </button>
          <button class="lightbox-close" onclick={closeLightbox}>
            <Icon name="close" size={20} />
          </button>
        </div>
      </div>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <img src={lightboxSrc} alt={lightboxName} class="lightbox-img" class:lightbox-img-loading={lightboxLoading} onclick={(e) => e.stopPropagation()} />
      {#if lightboxLoading}
        <span class="lightbox-loading">Loading full image...</span>
      {/if}
    </div>
  {/if}

  {#if filePreview || filePreviewLoading}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="file-preview-overlay" onclick={closeFilePreview}>
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="file-preview-modal" onclick={(e) => e.stopPropagation()}>
        <div class="file-preview-header">
          <div class="file-preview-title">
            <span class="file-preview-name">{filePreview?.name ?? '...'}</span>
            {#if filePreview}
              <span class="file-preview-meta">
                {filePreview.size} &bull; {filePreview.line_count} lines
                {#if filePreview.truncated} &bull; Truncated{/if}
              </span>
            {/if}
          </div>
          <div class="file-preview-actions">
            <button class="lightbox-btn" onclick={() => showInExplorer(filePreviewPath)} title="Open in folder">
              <Icon name="folder_open" size={18} />
            </button>
            <button class="lightbox-close" onclick={closeFilePreview}>
              <Icon name="close" size={20} />
            </button>
          </div>
        </div>
        <div class="file-preview-body">
          {#if filePreviewLoading}
            <div class="file-preview-loading">Loading preview...</div>
          {:else if filePreview?.content}
            <pre class="file-preview-code">{filePreview.content}</pre>
          {:else}
            <div class="file-preview-loading">No preview available for this file type.</div>
          {/if}
        </div>
      </div>
    </div>
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
    padding: 8px 12px 80px;
    display: flex;
    flex-direction: column;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }

  .msg-row {
    display: flex;
    margin-top: 2px;
  }
  .msg-row-sent { justify-content: flex-end; }
  .msg-row-received { justify-content: flex-start; }
  .msg-group-break { margin-top: 8px; }
  .msg-row:first-child { margin-top: 0; }

  .bubble {
    position: relative;
    max-width: clamp(140px, 78%, 400px);
    width: fit-content;
    padding: 6px 10px 4px;
    cursor: pointer;
    overflow-wrap: break-word;
    word-break: break-word;
    animation: bubble-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  .bubble-media { padding: 4px 4px 4px; }
  @keyframes bubble-in {
    from { opacity: 0; transform: translateY(3px) scale(0.98); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .bubble-sent {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .bubble-received {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  .bubble-sent.bubble-solo   { border-radius: 18px 18px 4px 18px; }
  .bubble-sent.bubble-first  { border-radius: 18px 18px 4px 18px; }
  .bubble-sent.bubble-middle { border-radius: 18px 4px 4px 18px; }
  .bubble-sent.bubble-last   { border-radius: 18px 4px 18px 18px; }

  .bubble-received.bubble-solo   { border-radius: 18px 18px 18px 4px; }
  .bubble-received.bubble-first  { border-radius: 18px 18px 18px 4px; }
  .bubble-received.bubble-middle { border-radius: 4px 18px 18px 4px; }
  .bubble-received.bubble-last   { border-radius: 4px 18px 18px 18px; }

  .bubble::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s ease;
  }
  .bubble:hover::after { opacity: 0.05; }
  .bubble:active::after { opacity: 0.08; }

  .bubble-copied { animation: bubble-flash 0.2s ease; }
  @keyframes bubble-flash { from { opacity: 0.7; } to { opacity: 1; } }

  .bubble-contact { font-size: 10px; font-weight: 500; color: var(--md-sys-color-primary); margin-bottom: 1px; }
  .bubble-text {
    font-family: var(--md-sys-typescale-body-small-font, "Roboto Mono", monospace);
    font-size: 13px;
    line-height: 1.4;
    white-space: pre-wrap;
    user-select: text;
  }
  .bubble-time-spacer { display: inline-block; width: 58px; height: 1px; }
  .bubble-meta { position: absolute; right: 8px; bottom: 3px; display: flex; align-items: center; gap: 3px; }
  .bubble-time { font-size: 10px; line-height: 1; opacity: 0.45; white-space: nowrap; }
  .bubble-copied-badge {
    display: flex; align-items: center; gap: 2px; font-size: 10px; font-weight: 500;
    color: var(--md-sys-color-primary);
    animation: badge-pop var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes badge-pop { from { transform: scale(0.8); opacity: 0; } to { transform: scale(1); opacity: 1; } }

  .star-btn {
    display: flex; align-items: center; background: transparent; border: none;
    cursor: pointer; padding: 0; color: var(--md-sys-color-outline); opacity: 0;
    transition: opacity 0.15s ease, color 0.15s ease;
  }
  .star-btn.starred { opacity: 1; color: var(--md-sys-color-primary); }
  .bubble:hover .star-btn { opacity: 0.7; }
  .bubble:hover .star-btn.starred { opacity: 1; }

  .att-images { margin-bottom: 4px; border-radius: 12px; overflow: hidden; }
  .att-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 3px; }
  .att-img-wrap { position: relative; background: var(--md-sys-color-surface-container); min-height: 60px; }
  .att-img { display: block; width: 100%; max-height: 200px; object-fit: cover; cursor: pointer; }
  .att-img-placeholder {
    display: flex; align-items: center; justify-content: center;
    width: 100%; min-height: 80px;
    color: var(--md-sys-color-on-surface-variant);
    animation: shimmer 1.5s ease-in-out infinite alternate;
  }
  .att-grid .att-img { aspect-ratio: 1; max-height: none; }

  .att-files { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 4px; }
  .att-file-card {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 140px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 25%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 4%, transparent);
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .att-file-card:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent);
  }
  .att-file-name {
    font-size: 12px;
    font-weight: 600;
    line-height: 1.3;
    word-break: break-word;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .att-file-size {
    font-size: 11px;
    opacity: 0.5;
  }
  .att-file-badge {
    display: inline-block;
    margin-top: 6px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    width: fit-content;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    color: var(--md-sys-color-on-surface-variant);
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

  .composer {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    max-height: 70%;
    padding: 8px 12px 12px;
    display: flex;
    flex-direction: column;
    pointer-events: none;
    z-index: 10;
  }

  .composer-box {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    border-radius: 12px;
    border: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    transition: border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
    cursor: text;
    overflow: hidden;
    pointer-events: auto;
  }
  .composer-box:focus-within {
    border-color: var(--md-sys-color-primary);
  }

  .composer-actions {
    display: flex;
    align-items: center;
    padding: 4px 6px 6px;
    flex-shrink: 0;
  }

  .composer-attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 12px 4px;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }

  /* ── Image previews inside composer ── */
  .composer-img {
    position: relative;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    overflow: hidden;
    cursor: pointer;
    flex-shrink: 0;
  }
  .composer-img::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s ease;
  }
  .composer-img:hover::after { opacity: 0.08; }

  .composer-img-preview {
    display: block;
    width: 56px;
    height: 56px;
    object-fit: cover;
    border-radius: 8px;
  }

  .composer-img-loading {
    width: 56px;
    height: 56px;
    border-radius: 8px;
    background: var(--md-sys-color-surface-container);
    overflow: hidden;
    position: relative;
  }
  .composer-img-shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent) 50%,
      transparent 100%
    );
    animation: shimmer-slide 1.5s ease-in-out infinite;
  }
  @keyframes shimmer-slide {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  .composer-img-remove {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease;
    padding: 0;
    z-index: 1;
  }
  .composer-img:hover .composer-img-remove { opacity: 1; }

  .composer-file-card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 120px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 25%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 4%, transparent);
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s ease;
  }
  .composer-file-card:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent);
  }
  .composer-file-name {
    font-size: 12px;
    font-weight: 600;
    line-height: 1.3;
    word-break: break-word;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .composer-file-size {
    font-size: 11px;
    opacity: 0.5;
  }
  .composer-file-remove {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: none;
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease;
    padding: 0;
    z-index: 1;
  }
  .composer-file-card:hover .composer-file-remove { opacity: 1; }
  .composer-file-remove:hover { color: var(--md-sys-color-error); }

  /* ── Textarea ── */
  .composer-textarea {
    display: block;
    width: 100%;
    box-sizing: border-box;
    border: none;
    outline: none;
    background: transparent;
    color: var(--md-sys-color-on-surface);
    font-family: var(--font-plain);
    font-size: 14px;
    line-height: 20px;
    padding: 10px 16px 4px;
    resize: none;
    min-height: 20px;
    max-height: 120px;
    overflow-y: auto;
    field-sizing: content;
    scrollbar-width: thin;
    word-break: break-word;
    overflow-wrap: break-word;
    flex-shrink: 1;
  }
  .composer-textarea::placeholder {
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
  }

  /* ── FAB button — 40dp, left side, from fab-menu.md ── */
  .fab-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: none;
    cursor: pointer;
    position: relative;
    overflow: hidden;
    box-shadow: var(--shadow-level3);
  }
  .fab-btn::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-primary-container);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .fab-btn:hover::after { opacity: 0.08; }
  .fab-btn:active::after { opacity: 0.1; }
  .fab-btn:hover { box-shadow: var(--shadow-level4); }

  /* ── Send button — 56dp, right side, mirrors FAB ── */
  .send-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    border: none;
    flex-shrink: 0;
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    cursor: default;
    opacity: 0.4;
    box-shadow: var(--shadow-level3);
    position: relative;
    overflow: hidden;
    transition:
      opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .send-btn::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-primary-container);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .send-active {
    cursor: pointer;
    opacity: 1;
  }
  .send-btn:hover::after { opacity: 0.08; }
  .send-btn:active::after { opacity: 0.1; }

  /* ── FAB menu items — pills float above the composer ── */
  .fab-menu-items {
    position: absolute;
    bottom: calc(100% + 4px);
    left: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 10;
    pointer-events: auto;
  }
  .fab-menu-item {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 40px;
    padding: 0 16px;
    border-radius: 9999px;
    border: none;
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    box-shadow: var(--shadow-level3);
    cursor: pointer;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 500;
    position: relative;
    overflow: hidden;
    animation: fab-menu-enter var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  .fab-menu-item::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-primary-container);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .fab-menu-item:hover::after { opacity: 0.08; }
  .fab-menu-item:active::after { opacity: 0.1; }
  @keyframes fab-menu-enter {
    from { opacity: 0; transform: translateY(8px) scale(0.92); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .drag-overlay {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, var(--md-sys-color-surface) 88%);
    color: var(--md-sys-color-primary);
    font-size: 14px;
    font-weight: 500;
    border-radius: 12px;
    pointer-events: none;
    animation: overlay-in 0.15s ease both;
  }
  @keyframes overlay-in {
    from { opacity: 0; }
    to { opacity: 1; }
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
    animation: menu-in 0.15s ease both;
  }
  @keyframes menu-in {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: translateY(0); }
  }
  .clear-menu button {
    display: block; width: 100%; text-align: left;
    padding: 8px 12px; background: transparent; border: none;
    color: var(--md-sys-color-on-surface); font-size: 12px;
    cursor: pointer; transition: background 0.15s ease;
  }
  .clear-menu button:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }

  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.85);
    animation: overlay-in 0.15s ease both;
    cursor: pointer;
  }
  .lightbox-header {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: linear-gradient(rgba(0,0,0,0.6), transparent);
  }
  .lightbox-name {
    font-size: 12px;
    color: rgba(255,255,255,0.8);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .lightbox-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .lightbox-btn, .lightbox-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 50%;
    background: rgba(255,255,255,0.15);
    color: #fff;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s ease;
  }
  .lightbox-btn:hover, .lightbox-close:hover {
    background: rgba(255,255,255,0.25);
  }
  .lightbox-img {
    max-width: 90%;
    max-height: 80%;
    object-fit: contain;
    border-radius: 8px;
    cursor: default;
    transition: opacity 0.2s ease;
  }
  .lightbox-img-loading {
    opacity: 0.5;
    filter: blur(2px);
  }
  .lightbox-loading {
    margin-top: 8px;
    font-size: 11px;
    color: rgba(255,255,255,0.5);
  }

  /* ── File preview modal ── */
  .file-preview-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    animation: overlay-in 0.15s ease both;
    cursor: pointer;
  }
  .file-preview-modal {
    display: flex;
    flex-direction: column;
    width: 90%;
    max-width: 700px;
    max-height: 80%;
    border-radius: 16px;
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface);
    box-shadow: 0 8px 32px rgba(0,0,0,0.4);
    cursor: default;
    overflow: hidden;
    animation: dialog-scale-in 0.2s ease both;
  }
  @keyframes dialog-scale-in {
    from { transform: scale(0.95); opacity: 0; }
    to   { transform: scale(1); opacity: 1; }
  }
  .file-preview-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 16px 16px 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 20%, transparent);
    flex-shrink: 0;
  }
  .file-preview-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .file-preview-name {
    font-size: 16px;
    font-weight: 600;
    word-break: break-word;
  }
  .file-preview-meta {
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.7;
  }
  .file-preview-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    margin-left: 12px;
  }
  .file-preview-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    scrollbar-width: thin;
  }
  .file-preview-code {
    margin: 0;
    padding: 16px;
    font-family: var(--md-sys-typescale-body-small-font, "Roboto Mono", monospace);
    font-size: 13px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 4%, transparent);
    border-radius: 0 0 16px 16px;
  }
  .file-preview-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px 20px;
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.6;
  }
</style>
