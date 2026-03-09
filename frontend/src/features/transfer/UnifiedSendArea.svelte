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

  // Auto-scroll messages to bottom when new messages arrive
  $effect(() => {
    if (displayMessages.length && messagesEl && !showStarredOnly && !app.messageSearch) {
      requestAnimationFrame(() => {
        if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
      });
    }
  });

  async function handleCopy(msgId: string, text: string) {
    await copyToClipboard(text);
    copiedMsgId = msgId;
    setTimeout(() => { if (copiedMsgId === msgId) copiedMsgId = null; }, 1200);
    onsnackbar?.("Copied to clipboard");
  }

  function getContactName(contactId: string): string {
    return app.contacts.find(c => c.id === contactId)?.name ?? "Unknown";
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
      <span class="text-sm font-medium text-on-surface-variant flex-1">Message</span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={msgOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </div>

    {#if msgOpen}
      <div class="section-enter">
        <!-- Chat toolbar: All texts toggle, starred filter, search, clear -->
        {#if app.messages.length > 0}
          <div class="flex items-center gap-1 mt-3 mb-1 flex-wrap">
            <!-- All texts / This contact toggle -->
            <button
              class="text-xs px-2 py-1 rounded-full border-none cursor-pointer
                     {app.messageViewAll
                       ? 'bg-primary text-on-primary'
                       : 'bg-surface-container-high text-on-surface-variant'}"
              style="transition: all var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
              onclick={() => app.messageViewAll = !app.messageViewAll}
            >
              {app.messageViewAll ? "All contacts" : "This contact"}
            </button>

            <!-- Starred filter -->
            <button
              class="text-xs px-2 py-1 rounded-full border-none cursor-pointer
                     {showStarredOnly
                       ? 'bg-primary text-on-primary'
                       : 'bg-surface-container-high text-on-surface-variant'}"
              style="transition: all var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
              onclick={() => showStarredOnly = !showStarredOnly}
            >
              <Icon name={showStarredOnly ? "star" : "star_border"} size={12} />
              Saved
            </button>

            <span class="flex-1"></span>

            <!-- Search icon toggle -->
            <button
              class="text-xs text-on-surface-variant cursor-pointer bg-transparent border-none px-1 py-0.5"
              onclick={() => { app.messageSearch = app.messageSearch ? "" : " "; }}
              title="Search messages"
            >
              <Icon name="search" size={16} />
            </button>

            <!-- Clear menu -->
            <div class="relative">
              <button
                class="text-xs text-on-surface-variant cursor-pointer bg-transparent border-none px-1 py-0.5
                       hover:text-error"
                style="transition: color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
                onclick={() => showClearMenu = !showClearMenu}
              >
                Clear
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
          {#if app.messageSearch !== ""}
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
              class="flex flex-col gap-1 max-h-52 overflow-y-auto msg-scroll"
            >
              {#each displayMessages as msg (msg.id)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="msg-bubble rounded-lg msg-row cursor-pointer
                         {msg.direction === 'sent' ? 'msg-sent' : 'msg-received'}
                         {copiedMsgId === msg.id ? 'msg-copied' : ''}"
                  onclick={() => handleCopy(msg.id, msg.text)}
                  title="Click to copy"
                >
                  <div class="px-3 py-2">
                    {#if app.messageViewAll}
                      <div class="text-[10px] text-on-surface-variant mb-0.5">
                        {getContactName(msg.contactId)}
                      </div>
                    {/if}
                    <div class="font-mono text-xs leading-relaxed whitespace-pre-wrap break-all select-text">
                      {msg.text}
                    </div>
                    <div class="flex items-center gap-1 mt-0.5 justify-end msg-footer">
                      {#if copiedMsgId === msg.id}
                        <span class="text-primary msg-check"><Icon name="check" size={10} /></span>
                        <span class="text-[10px] text-primary font-medium">Copied</span>
                      {:else}
                        <!-- Star button -->
                        <button
                          class="star-btn"
                          class:starred={msg.starred}
                          onclick={(e) => { e.stopPropagation(); app.toggleStar(msg.id); }}
                          title={msg.starred ? "Unsave" : "Save"}
                        >
                          <Icon name={msg.starred ? "star" : "star_border"} size={12} />
                        </button>
                        <span class="text-[10px] opacity-40">
                          {new Date(msg.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>
              {/each}
            </div>
          {:else if app.messageSearch || showStarredOnly}
            <div class="text-xs text-on-surface-variant text-center py-3">
              {showStarredOnly ? "No saved messages" : "No results"}
            </div>
          {/if}
        {/if}

        <!-- Text input — always visible when section is open -->
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
  .msg-sent {
    background-color: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
    color: var(--md-sys-color-on-surface);
    margin-left: 1.5rem;
  }
  .msg-received {
    background-color: color-mix(in srgb, var(--md-sys-color-tertiary) 12%, transparent);
    color: var(--md-sys-color-on-surface);
    margin-right: 1.5rem;
  }
  .msg-bubble {
    position: relative;
    overflow: hidden;
    transition: background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  /* M3 state layer: 8% hover, 10% press */
  .msg-bubble::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .msg-bubble:hover::after { opacity: 0.08; }
  .msg-bubble:active::after { opacity: 0.1; }
  .msg-copied {
    animation: msg-flash var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  @keyframes msg-flash {
    from { opacity: 0.7; }
    to   { opacity: 1; }
  }
  .msg-check {
    animation: check-pop var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes check-pop {
    from { transform: scale(0.8); opacity: 0; }
    to   { transform: scale(1); opacity: 1; }
  }
  .msg-row {
    animation: msg-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes msg-in {
    from { opacity: 0; transform: translateY(4px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .msg-footer {
    height: 1rem;
    min-height: 1rem;
  }
  .msg-scroll {
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }
  .star-btn {
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    color: var(--md-sys-color-outline);
    opacity: 0;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
                color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
    display: flex;
    align-items: center;
  }
  .star-btn.starred {
    opacity: 1;
    color: var(--md-sys-color-primary);
  }
  .msg-bubble:hover .star-btn {
    opacity: 1;
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
