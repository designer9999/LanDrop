<!--
  Claude-style chat interface — messages + bottom composer with inline file attachments.
  No separate drop zone or file section. Everything flows through the composer.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { MessageAttachment } from "$lib/state/app-state.svelte";
  import { pickFiles, pickFolder, getFileInfo, copyToClipboard, getThumbnail, getFullImage } from "$lib/api/bridge";

  import { isImage } from "$lib/utils/file-utils";

  // Cache for base64 image thumbnails ("loading" = in progress, string = data URI)
  let thumbCache = $state<Record<string, string>>({});
  const _thumbLoading = new Set<string>(); // non-reactive loading tracker

  interface Props {
    contactName?: string;
    onsnackbar?: (msg: string) => void;
    onsend?: () => void;
    onsendtext?: () => void;
  }

  let { contactName, onsnackbar, onsend, onsendtext }: Props = $props();

  const app = getAppState();

  let messagesEl: HTMLDivElement | undefined = $state();
  let copiedMsgId = $state<string | null>(null);
  let showStarredOnly = $state(false);
  let showClearMenu = $state(false);
  let searchOpen = $state(false);
  let dragOver = $state(false);
  let composerEl: HTMLTextAreaElement | undefined = $state();

  // Lightbox for full-size image preview
  let lightboxSrc = $state<string | null>(null);
  let lightboxName = $state("");
  let lightboxLoading = $state(false);

  async function openLightbox(path: string, name: string) {
    lightboxName = name;
    lightboxLoading = true;
    lightboxSrc = thumbCache[path] ?? null; // Show thumbnail immediately while loading
    const full = await getFullImage(path, 800);
    lightboxLoading = false;
    if (full) lightboxSrc = full;
  }

  function closeLightbox() {
    lightboxSrc = null;
    lightboxName = "";
    lightboxLoading = false;
  }

  // Filtered messages for display
  const displayMessages = $derived.by(() => {
    const contactId = app.messageViewAll ? undefined : app.activeContact?.id;
    if (!contactId && !app.messageViewAll) return [];

    let msgs: import("$lib/state/app-state.svelte").MessageEntry[];

    if (showStarredOnly) {
      msgs = app.getStarredMessages(contactId);
    } else if (app.messageSearch) {
      msgs = app.searchMessages(app.messageSearch, contactId);
    } else if (contactId) {
      msgs = app.getContactMessages(contactId);
    } else {
      msgs = [...app.messages];
    }
    return msgs;
  });

  // Show toolbar if current view has messages OR we're in starred/search mode
  const currentContactMessages = $derived(
    app.messageViewAll
      ? app.messages
      : app.activeContact
        ? app.getContactMessages(app.activeContact.id)
        : []
  );
  const showToolbar = $derived(currentContactMessages.length > 0 || showStarredOnly || searchOpen);

  // Auto-scroll to bottom
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

  // Message grouping
  function getBubblePosition(index: number): "solo" | "first" | "middle" | "last" {
    const msgs = displayMessages;
    const curr = msgs[index];
    const prev = index > 0 ? msgs[index - 1] : null;
    const next = index < msgs.length - 1 ? msgs[index + 1] : null;
    const sameAsPrev = prev && prev.direction === curr.direction && prev.contactId === curr.contactId;
    const sameAsNext = next && next.direction === curr.direction && next.contactId === curr.contactId;
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

  function getContactName(contactId: string): string {
    return app.contacts.find(c => c.id === contactId)?.name ?? "Unknown";
  }

  function formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  // Load thumbnails lazily — one at a time to avoid IPC flood
  function loadThumb(path: string) {
    if (path in thumbCache || _thumbLoading.has(path)) return;
    _thumbLoading.add(path);
    getThumbnail(path).then(uri => {
      _thumbLoading.delete(path);
      thumbCache = { ...thumbCache, [path]: uri ?? "" }; // "" = no thumbnail available
    }).catch(() => {
      _thumbLoading.delete(path);
      thumbCache = { ...thumbCache, [path]: "" };
    });
  }

  // Trigger loading for visible images
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

  // File picking
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

  // Drag & drop on chat area
  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    // pywebview's native handler will fire files_dropped event
    // No stopPropagation — let it bubble
  }

  // Send handler (calls parent)
  function handleSend() {
    if (app.hasFiles) {
      onsend?.();
    } else if (app.sendTextContent.trim()) {
      onsendtext?.();
    }
  }

  // Enter to send (Shift+Enter for newline)
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  // Computed
  const canSend = $derived(!app.transferActive && (app.hasFiles || !!app.sendTextContent.trim()));
  const imageFiles = $derived(app.files.filter(f => f.info && isImage(f.info.type)));
  const otherFiles = $derived(app.files.filter(f => !f.info || !isImage(f.info.type)));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="chat-container"
  class:drag-over={dragOver}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  <!-- Chat toolbar -->
  {#if showToolbar}
    <div class="chat-toolbar">
      <button
        class="chip"
        class:chip-active={app.messageViewAll}
        onclick={() => app.messageViewAll = !app.messageViewAll}
      >
        {app.messageViewAll ? "All contacts" : "This contact"}
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
            <button onclick={() => { app.activeContact && app.clearMessages(app.activeContact.id); }}>
              Clear chat (keep saved)
            </button>
            <button onclick={() => { app.activeContact && app.deleteAllMessages(app.activeContact.id); }}>
              Delete all for this contact
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

  <!-- Message area (scrollable, fills space) -->
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
              <div class="bubble-contact">{getContactName(msg.contactId)}</div>
            {/if}

            <!-- Image attachments (clickable for full-size) -->
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

            <!-- File attachments -->
            {#if files.length > 0}
              <div class="att-files">
                {#each files as file}
                  <div class="att-file">
                    <Icon name="insert_drive_file" size={18} />
                    <div class="att-file-info">
                      <span class="att-file-name">{file.name}</span>
                      {#if file.size}<span class="att-file-size">{file.size}</span>{/if}
                    </div>
                  </div>
                {/each}
              </div>
            {/if}

            <!-- Text content -->
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
        <Icon name="chat_bubble_outline" size={36} />
        <span class="text-sm">{contactName ? `Send files or messages to ${contactName}` : "Select a contact to start"}</span>
        <span class="text-[10px] opacity-50">Drop files here, attach with clip, or type below</span>
      </div>
    {/if}
  </div>

  <!-- Composer (fixed at bottom of chat) -->
  <div class="composer">
    <!-- Attached files preview (Claude-style inline chips) -->
    {#if app.hasFiles}
      <div class="composer-attachments">
        {#each imageFiles as file (file.path)}
          <div class="att-thumb">
            {#if thumbCache[file.path] && thumbCache[file.path] !== ""}
              <img src={thumbCache[file.path]} alt={file.info?.name ?? "image"} class="att-thumb-img" />
            {:else}
              <div class="att-thumb-placeholder"><Icon name="image" size={16} /></div>
            {/if}
            {#if !app.transferActive}
              <button class="att-thumb-remove" onclick={() => app.removeFile(file.path)}>
                <Icon name="close" size={10} />
              </button>
            {/if}
          </div>
        {/each}
        {#each otherFiles as file (file.path)}
          <div class="att-chip">
            <Icon name={file.info?.type === "folder" ? "folder" : "insert_drive_file"} size={14} />
            <span class="att-chip-name">{file.info?.name ?? file.path.split(/[\\/]/).pop()}</span>
            {#if file.info?.size}
              <span class="att-chip-size">{file.info.size}</span>
            {/if}
            {#if !app.transferActive}
              <button class="att-chip-remove" onclick={() => app.removeFile(file.path)}>
                <Icon name="close" size={12} />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <!-- Input row -->
    <div class="composer-row">
      <div class="composer-actions">
        <button class="composer-btn" onclick={handlePickFiles} title="Attach files">
          <Icon name="attach_file" size={20} />
        </button>
        <button class="composer-btn" onclick={handlePickFolder} title="Attach folder">
          <Icon name="folder" size={20} />
        </button>
      </div>
      <div class="composer-input">
        <textarea
          bind:this={composerEl}
          class="composer-textarea"
          placeholder={contactName ? `Message ${contactName}...` : "Type a message..."}
          rows={1}
          bind:value={app.sendTextContent}
          onkeydown={handleKeydown}
        ></textarea>
      </div>
      <button
        class="composer-send"
        class:composer-send-active={canSend}
        disabled={!canSend}
        onclick={handleSend}
      >
        <Icon name="send" size={20} />
      </button>
    </div>
  </div>

  <!-- Drag overlay -->
  {#if dragOver}
    <div class="drag-overlay">
      <Icon name="upload_file" size={40} />
      <span>Drop files here</span>
    </div>
  {/if}

  <!-- Image lightbox -->
  {#if lightboxSrc}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="lightbox" onclick={closeLightbox}>
      <div class="lightbox-header">
        <span class="lightbox-name">{lightboxName}</span>
        <button class="lightbox-close" onclick={closeLightbox}>
          <Icon name="close" size={20} />
        </button>
      </div>
      <img src={lightboxSrc} alt={lightboxName} class="lightbox-img" class:lightbox-img-loading={lightboxLoading} />
      {#if lightboxLoading}
        <span class="lightbox-loading">Loading full image...</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* ── Container — fills available space ── */
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

  /* ── Chat Toolbar ── */
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

  /* ── Messages area — scrollable, fills space ── */
  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }

  /* ── Message Row ── */
  .msg-row {
    display: flex;
    margin-top: 2px;
  }
  .msg-row-sent { justify-content: flex-end; }
  .msg-row-received { justify-content: flex-start; }
  .msg-group-break { margin-top: 8px; }
  .msg-row:first-child { margin-top: 0; }

  /* ── Bubble ── */
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

  /* ── Bubble Content ── */
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

  /* ── Star Button ── */
  .star-btn {
    display: flex; align-items: center; background: transparent; border: none;
    cursor: pointer; padding: 0; color: var(--md-sys-color-outline); opacity: 0;
    transition: opacity 0.15s ease, color 0.15s ease;
  }
  .star-btn.starred { opacity: 1; color: var(--md-sys-color-primary); }
  .bubble:hover .star-btn { opacity: 0.7; }
  .bubble:hover .star-btn.starred { opacity: 1; }

  /* ── Attachments in bubbles ── */
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

  .att-files { display: flex; flex-direction: column; gap: 4px; margin-bottom: 4px; }
  .att-file { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 8px; background: color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent); }
  .att-file-info { display: flex; flex-direction: column; min-width: 0; }
  .att-file-name { font-size: 12px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .att-file-size { font-size: 10px; opacity: 0.5; }

  /* ── Empty State ── */
  .chat-empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 6px; flex: 1; padding: 40px 20px;
    color: var(--md-sys-color-on-surface-variant); opacity: 0.4;
    font-size: 12px; text-align: center;
  }

  /* ── Composer ── */
  .composer {
    flex-shrink: 0;
    border-top: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    padding: 0;
  }

  /* Attached files in composer */
  .composer-attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 12px 0;
  }

  /* Image thumbnail chips */
  .att-thumb {
    position: relative;
    width: 48px;
    height: 48px;
    border-radius: 8px;
    overflow: hidden;
    background: var(--md-sys-color-surface-container);
    flex-shrink: 0;
  }
  .att-thumb-img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .att-thumb-placeholder {
    width: 100%; height: 100%; display: flex; align-items: center; justify-content: center;
    color: var(--md-sys-color-on-surface-variant); opacity: 0.4;
    animation: shimmer 1.5s ease-in-out infinite alternate;
  }
  @keyframes shimmer {
    from { opacity: 0.2; }
    to   { opacity: 0.5; }
  }
  .att-thumb-remove {
    position: absolute; top: 2px; right: 2px; width: 16px; height: 16px;
    border-radius: 50%; border: none; background: rgba(0,0,0,0.6); color: #fff;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; opacity: 0; transition: opacity 0.15s ease; padding: 0;
  }
  .att-thumb:hover .att-thumb-remove { opacity: 1; }

  /* File chips in composer */
  .att-chip {
    display: flex; align-items: center; gap: 4px;
    padding: 4px 8px; border-radius: 8px; height: 28px;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
    font-size: 11px; max-width: 180px;
  }
  .att-chip-name {
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    font-weight: 500; flex: 1; min-width: 0;
  }
  .att-chip-size { font-size: 10px; opacity: 0.5; flex-shrink: 0; }
  .att-chip-remove {
    display: flex; align-items: center; background: transparent; border: none;
    cursor: pointer; padding: 0; color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5; flex-shrink: 0;
  }
  .att-chip-remove:hover { opacity: 1; color: var(--md-sys-color-error); }

  /* Composer input row */
  .composer-row {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding: 6px 4px 6px 4px;
  }
  .composer-actions {
    display: flex;
    flex-shrink: 0;
  }
  .composer-btn {
    display: flex; align-items: center; justify-content: center;
    width: 36px; height: 36px; border: none; border-radius: 50%;
    background: transparent; color: var(--md-sys-color-on-surface-variant);
    cursor: pointer; transition: color 0.15s ease, background 0.15s ease;
  }
  .composer-btn:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    color: var(--md-sys-color-on-surface);
  }

  .composer-input {
    flex: 1;
    min-width: 0;
  }
  .composer-textarea {
    width: 100%;
    border: none;
    outline: none;
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface);
    font-family: var(--md-sys-typescale-body-small-font, "Roboto Mono", monospace);
    font-size: 13px;
    line-height: 1.4;
    padding: 8px 12px;
    border-radius: 18px;
    resize: none;
    max-height: 120px;
    overflow-y: auto;
    field-sizing: content;
    scrollbar-width: thin;
  }
  .composer-textarea::placeholder {
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
  }

  .composer-send {
    display: flex; align-items: center; justify-content: center;
    width: 36px; height: 36px; border: none; border-radius: 50%;
    background: transparent; color: var(--md-sys-color-on-surface-variant);
    cursor: default; transition: all 0.15s ease; flex-shrink: 0; opacity: 0.4;
  }
  .composer-send-active {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    cursor: pointer;
    opacity: 1;
  }
  .composer-send-active:hover {
    box-shadow: 0 1px 3px rgba(0,0,0,0.3);
  }

  /* ── Drag Overlay ── */
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

  /* ── Clear Menu ── */
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

  /* ── Lightbox (full-size image overlay) ── */
  .lightbox {
    position: absolute;
    inset: 0;
    z-index: 30;
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
  }
  .lightbox-close {
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
  }
  .lightbox-close:hover {
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
</style>
