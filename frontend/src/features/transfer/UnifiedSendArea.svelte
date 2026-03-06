<!--
  Unified send area — files + message input + chat history
  Reuses DropZone, FileList from send/ components
-->
<script lang="ts">
  import Card from "$lib/ui/Card.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Icon from "$lib/ui/Icon.svelte";
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

  // Auto-scroll messages to bottom when new messages arrive
  $effect(() => {
    if (app.activeContact) {
      const msgs = app.getContactMessages(app.activeContact.id);
      if (msgs.length && messagesEl) {
        requestAnimationFrame(() => {
          if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
        });
      }
    }
  });

  async function handleCopy(text: string) {
    await copyToClipboard(text);
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
        <!-- Chat history -->
        {#if app.activeContact}
          {@const msgs = app.getContactMessages(app.activeContact.id)}
          {#if msgs.length > 0}
            <div class="flex items-center gap-2 mt-3 mb-1">
              <span class="text-xs text-on-surface-variant flex-1">Chat</span>
              <button
                class="text-xs text-on-surface-variant cursor-pointer bg-transparent border-none px-1 py-0.5
                       hover:text-error"
                style="transition: color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
                onclick={() => app.activeContact && app.clearMessages(app.activeContact.id)}
              >
                Clear
              </button>
            </div>

            <div
              bind:this={messagesEl}
              class="flex flex-col gap-1 max-h-52 overflow-y-auto msg-scroll"
            >
              {#each msgs as msg (msg.id)}
                <div
                  class="group/msg flex items-start gap-0 rounded-lg msg-row
                         {msg.direction === 'sent' ? 'msg-sent' : 'msg-received'}"
                >
                  <div class="flex-1 min-w-0 px-3 py-2">
                    <div class="font-mono text-xs leading-relaxed whitespace-pre-wrap break-all select-text">
                      {msg.text}
                    </div>
                    <div class="text-[10px] opacity-40 mt-0.5 text-right">
                      {new Date(msg.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                    </div>
                  </div>
                  <button
                    class="shrink-0 w-7 h-7 inline-flex items-center justify-center rounded-full
                           opacity-0 group-hover/msg:opacity-100 mt-1.5 mr-1
                           cursor-pointer bg-transparent border-none text-inherit hover:bg-white/10"
                    style="transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
                    onclick={() => handleCopy(msg.text)}
                    title="Copy message"
                  >
                    <Icon name="content_copy" size={14} />
                  </button>
                </div>
              {/each}
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
  .msg-row {
    animation: msg-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes msg-in {
    from { opacity: 0; transform: translateY(4px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .msg-scroll {
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }
</style>
