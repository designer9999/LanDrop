<!--
  Unified send area — files + message input + chat history
  Reuses DropZone, FileList from send/ components
-->
<script lang="ts">
  import Card from "$lib/ui/Card.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { pickFiles, pickFolder, getFileInfo, copyToClipboard } from "$lib/api/bridge";
  import DropZone from "../send/DropZone.svelte";
  import FileList from "../send/FileList.svelte";

  interface Props {
    contactName?: string;
    onsnackbar?: (msg: string) => void;
  }

  let { contactName, onsnackbar }: Props = $props();

  const app = getAppState();

  let messagesEl: HTMLDivElement | undefined = $state();
  let msgOpen = $state(true);
  let copiedMsgId = $state<string | null>(null);
  let showStarredOnly = $state(false);
  let showClearMenu = $state(false);
  let searchOpen = $state(false);

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

  // Auto-scroll to bottom only when user is near the bottom
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

  // Message grouping: consecutive same-direction messages get visual grouping
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

  function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
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
</script>

<div class="transfer-wrap" class:transfer-active={app.transferActive}>
<Card variant="outlined">
  <div class="flex items-center gap-2 text-base font-medium text-on-surface mb-3">
    <span class="text-primary"><Icon name="send" size={20} /></span>
    {contactName ? `Send to ${contactName}` : "Send"}
  </div>

  <!-- Drop zone -->
  <DropZone onadd={addPaths} />

  <!-- Browse buttons -->
  <div class="flex gap-2 mt-3">
    <Button variant="tonal" full onclick={handlePickFiles}>
      <Icon name="description" size={18} />
      Files
    </Button>
    <Button variant="tonal" full onclick={handlePickFolder}>
      <Icon name="folder" size={18} />
      Folder
    </Button>
  </div>

  <!-- File list -->
  {#if app.hasFiles}
    <div class="mt-3 pt-3 border-t border-outline-variant">
      <div class="flex items-center gap-2 mb-2">
        <span class="text-sm text-on-surface font-medium">
          {app.files.length} {app.files.length === 1 ? "item" : "items"} selected
        </span>
        <span class="flex-1"></span>
        <button
          class="text-xs text-on-surface-variant px-2 py-1 rounded-sm
                 bg-transparent border-none
                 {app.transferActive ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer hover:text-error'}"
          style="transition: color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
          onclick={() => { if (!app.transferActive) app.clearFiles(); }}
          disabled={app.transferActive}
        >
          Clear all
        </button>
      </div>
      <FileList />
    </div>
  {/if}

  <!-- Message section — collapsible, open by default -->
  <div class="mt-3 pt-3 border-t border-outline-variant">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex items-center gap-2 cursor-pointer"
      onclick={() => msgOpen = !msgOpen}
    >
      <Icon name="chat" size={18} />
      <span class="text-sm font-medium text-on-surface-variant flex-1">Messages</span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={msgOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </div>

    {#if msgOpen}
      <div class="section-enter">
        <!-- Chat toolbar -->
        {#if app.messages.length > 0}
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
              <Icon name={showStarredOnly ? "star" : "star_border"} size={12} />
              Saved
            </button>

            <span class="flex-1"></span>

            <button
              class="toolbar-icon"
              class:toolbar-icon-active={searchOpen}
              onclick={() => { searchOpen = !searchOpen; if (!searchOpen) app.messageSearch = ""; }}
              title="Search messages"
            >
              <Icon name="search" size={16} />
            </button>

            <div class="relative">
              <button
                class="toolbar-icon hover:text-error"
                onclick={() => showClearMenu = !showClearMenu}
                title="Clear messages"
              >
                <Icon name="delete_outline" size={16} />
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

          <!-- Search input -->
          {#if searchOpen}
            <div class="mb-2">
              <TextField
                label="Search messages"
                placeholder="Type to search..."
                mono
                bind:value={app.messageSearch}
              />
            </div>
          {/if}

          <!-- Message list -->
          {#if displayMessages.length > 0}
            <div
              bind:this={messagesEl}
              class="chat-area"
              onscroll={handleScroll}
            >
              {#each displayMessages as msg, i (msg.id)}
                {@const pos = getBubblePosition(i)}
                {@const isSent = msg.direction === "sent"}
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
                    onclick={() => handleCopy(msg.id, msg.text)}
                    title="Click to copy"
                  >
                    {#if app.messageViewAll}
                      <div class="bubble-contact">{getContactName(msg.contactId)}</div>
                    {/if}
                    <span class="bubble-text">{msg.text}</span>
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
            </div>
          {:else if app.messageSearch || showStarredOnly}
            <div class="chat-empty">
              <Icon name={showStarredOnly ? "star_border" : "search_off"} size={32} />
              <span>{showStarredOnly ? "No saved messages" : "No results"}</span>
            </div>
          {/if}
        {:else}
          <div class="chat-empty mt-3">
            <Icon name="chat_bubble_outline" size={32} />
            <span>No messages yet</span>
          </div>
        {/if}

        <!-- Text input -->
        <div class="mt-3">
          <TextField
            label="Message"
            placeholder="Type text, URLs, commands..."
            multiline
            rows={3}
            mono
            bind:value={app.sendTextContent}
          />
        </div>
      </div>
    {/if}
  </div>

</Card>
</div>

<style>
  .transfer-wrap {
    border-radius: var(--md-sys-shape-corner-medium, 12px);
    padding: 2px;
    background: transparent;
    transition: background 0.3s ease;
  }
  .transfer-active {
    background: conic-gradient(
      from var(--transfer-angle, 0deg),
      var(--md-sys-color-tertiary) 0%,
      transparent 30%,
      transparent 70%,
      var(--md-sys-color-tertiary) 100%
    );
    animation: transfer-spin 1.5s linear infinite;
  }
  @keyframes transfer-spin {
    to { --transfer-angle: 360deg; }
  }
  @property --transfer-angle {
    syntax: "<angle>";
    initial-value: 0deg;
    inherits: false;
  }
  .section-arrow {
    transition: transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .section-arrow-open {
    transform: rotate(180deg);
  }
  .section-enter {
    animation: section-slide var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes section-slide {
    from { opacity: 0; transform: translateY(-8px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  /* ── Chat Toolbar ── */
  .chat-toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 12px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    padding: 3px 10px;
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
    padding: 4px;
    border-radius: 4px;
    color: var(--md-sys-color-on-surface-variant);
    transition: all var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .toolbar-icon-active {
    color: var(--md-sys-color-primary);
  }

  /* ── Chat Area ── */
  .chat-area {
    display: flex;
    flex-direction: column;
    max-height: 280px;
    overflow-y: auto;
    padding: 8px 4px;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }

  /* ── Message Row ── */
  .msg-row {
    display: flex;
    margin-top: 2px;
  }
  .msg-row-sent {
    justify-content: flex-end;
  }
  .msg-row-received {
    justify-content: flex-start;
  }
  .msg-group-break {
    margin-top: 10px;
  }
  .msg-row:first-child {
    margin-top: 0;
  }

  /* ── Bubble ── */
  .bubble {
    position: relative;
    max-width: clamp(140px, 78%, 400px);
    width: fit-content;
    padding: 7px 10px 4px;
    cursor: pointer;
    overflow-wrap: break-word;
    word-break: break-word;
    animation: bubble-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes bubble-in {
    from { opacity: 0; transform: translateY(4px) scale(0.97); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  /* Sent bubble — right side, primary tint */
  .bubble-sent {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  /* Received bubble — left side, surface */
  .bubble-received {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  /* Bubble grouping corner radius (sent = right side corners change) */
  .bubble-sent.bubble-solo   { border-radius: 18px 18px 4px 18px; }
  .bubble-sent.bubble-first  { border-radius: 18px 18px 4px 18px; }
  .bubble-sent.bubble-middle { border-radius: 18px 4px 4px 18px; }
  .bubble-sent.bubble-last   { border-radius: 18px 4px 18px 18px; }

  .bubble-received.bubble-solo   { border-radius: 18px 18px 18px 4px; }
  .bubble-received.bubble-first  { border-radius: 18px 18px 18px 4px; }
  .bubble-received.bubble-middle { border-radius: 4px 18px 18px 4px; }
  .bubble-received.bubble-last   { border-radius: 4px 18px 18px 18px; }

  /* Hover state layer */
  .bubble::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .bubble:hover::after { opacity: 0.06; }
  .bubble:active::after { opacity: 0.1; }

  .bubble-copied {
    animation: bubble-flash 0.2s ease;
  }
  @keyframes bubble-flash {
    from { opacity: 0.7; }
    to   { opacity: 1; }
  }

  /* ── Bubble Content ── */
  .bubble-contact {
    font-size: 10px;
    font-weight: 500;
    color: var(--md-sys-color-primary);
    margin-bottom: 2px;
  }
  .bubble-text {
    font-family: var(--md-sys-typescale-body-small-font, "Roboto Mono", monospace);
    font-size: 13px;
    line-height: 1.45;
    white-space: pre-wrap;
    user-select: text;
  }
  /* Invisible spacer to reserve room for the timestamp so text doesn't overlap */
  .bubble-time-spacer {
    display: inline-block;
    width: 64px;
    height: 1px;
  }
  .bubble-meta {
    position: absolute;
    right: 8px;
    bottom: 4px;
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .bubble-time {
    font-size: 10px;
    line-height: 1;
    opacity: 0.5;
    white-space: nowrap;
  }
  .bubble-copied-badge {
    display: flex;
    align-items: center;
    gap: 2px;
    font-size: 10px;
    font-weight: 500;
    color: var(--md-sys-color-primary);
    animation: badge-pop var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes badge-pop {
    from { transform: scale(0.8); opacity: 0; }
    to   { transform: scale(1); opacity: 1; }
  }

  /* ── Star Button ── */
  .star-btn {
    display: flex;
    align-items: center;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    color: var(--md-sys-color-outline);
    opacity: 0;
    transition: opacity 0.15s ease, color 0.15s ease;
  }
  .star-btn.starred {
    opacity: 1;
    color: var(--md-sys-color-primary);
  }
  .bubble:hover .star-btn {
    opacity: 0.7;
  }
  .bubble:hover .star-btn.starred {
    opacity: 1;
  }

  /* ── Empty State ── */
  .chat-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 24px 0;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
    font-size: 13px;
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
    animation: section-slide var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  .clear-menu button {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    background: transparent;
    border: none;
    color: var(--md-sys-color-on-surface);
    font-size: 12px;
    cursor: pointer;
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .clear-menu button:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
</style>
