<!--
  Settings page — receive defaults, about & status
-->
<script lang="ts">
  import Card from "$lib/ui/Card.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Switch from "$lib/ui/Switch.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { pickSaveFolder, copyToClipboard } from "$lib/api/bridge";

  interface Props {
    appVersion: string;
    onsnackbar?: (msg: string) => void;
  }

  let { appVersion, onsnackbar }: Props = $props();

  const app = getAppState();

  let receiveOpen = $state(true);
  let aboutOpen = $state(false);
  let debugOpen = $state(false);
  let debugEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (debugOpen && app.logs.length && debugEl) {
      requestAnimationFrame(() => {
        if (debugEl) debugEl.scrollTop = debugEl.scrollHeight;
      });
    }
  });
</script>

<div class="flex flex-col gap-4">

  <!-- Receive defaults -->
  <Card variant="outlined">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0 -my-0.5"
      onclick={() => receiveOpen = !receiveOpen}
    >
      <span class="text-primary"><Icon name="download" size={20} /></span>
      Receive settings
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={receiveOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if receiveOpen}
      <div class="flex flex-col gap-4 mt-4 section-enter" style="--tf-bg: var(--color-surface);">
        <div>
          <p class="text-xs font-medium text-on-surface-variant mb-2">Default save folder</p>
          {#if app.receiveOptions.outFolder}
            <div class="flex items-center gap-2 h-10 px-3 bg-surface-container-lowest border border-outline-variant rounded-xs">
              <span class="text-tertiary"><Icon name="folder" size={16} /></span>
              <span class="flex-1 text-xs text-on-surface font-mono truncate">{app.receiveOptions.outFolder}</span>
              <button
                class="w-7 h-7 inline-flex items-center justify-center rounded-full
                       text-on-surface-variant hover:text-error cursor-pointer bg-transparent border-none"
                onclick={() => app.updateReceiveOption("outFolder", undefined)}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          {:else}
            <Button variant="outlined" full onclick={async () => {
              const f = await pickSaveFolder();
              if (f) app.updateReceiveOption("outFolder", f);
            }}>
              <Icon name="create_new_folder" size={18} />
              Set save folder
            </Button>
          {/if}
        </div>

        <div class="flex items-center justify-between py-1">
          <div><div class="text-sm text-on-surface">Overwrite files</div><div class="text-xs text-on-surface-variant">Replace existing files without asking</div></div>
          <Switch checked={app.receiveOptions.overwrite ?? false} onchange={(v) => app.updateReceiveOption("overwrite", v || undefined)} />
        </div>
      </div>
    {/if}
  </Card>

  <!-- Notifications -->
  <Card variant="outlined">
    <div class="flex items-center gap-2 text-on-surface text-sm font-medium">
      <span class="text-primary"><Icon name="notifications" size={20} /></span>
      Notifications
    </div>
    <div class="flex items-center justify-between py-1 mt-3">
      <div>
        <div class="text-sm text-on-surface">Notification sounds</div>
        <div class="text-xs text-on-surface-variant">Play sound when files or messages arrive</div>
      </div>
      <Switch checked={app.notificationsEnabled} onchange={(v) => app.setNotifications(v)} />
    </div>
  </Card>

  <!-- About -->
  <Card variant="filled">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0"
      onclick={() => aboutOpen = !aboutOpen}
    >
      <span class="text-primary"><Icon name="info" size={20} /></span>
      About
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={aboutOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if aboutOpen}
      <div class="flex flex-col gap-3 mt-4 section-enter">
        <div class="flex items-center gap-3 p-4 rounded-xl bg-surface-container">
          <div class="flex items-center justify-center w-10 h-10 rounded-lg bg-primary">
            <span class="text-on-primary" style="font-size: 18px;"><Icon name="swap_horiz" size={20} /></span>
          </div>
          <div class="flex-1">
            <div class="text-sm text-on-surface font-medium">LanDrop v{appVersion}</div>
            <div class="text-xs text-on-surface-variant font-mono">{app.localIp}</div>
          </div>
        </div>

        <div class="text-sm text-on-surface-variant leading-relaxed">
          <p class="font-medium text-on-surface mb-1">How it works</p>
          <ol class="list-decimal list-inside flex flex-col gap-1">
            <li>Add a peer with a shared code phrase</li>
            <li>Both devices on the same network auto-connect</li>
            <li>Drop files or type messages — transferred instantly</li>
            <li>No internet needed — everything stays on your LAN</li>
          </ol>
        </div>
      </div>
    {/if}
  </Card>

  <!-- Debug Log -->
  <Card variant="outlined">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0 -my-0.5"
      onclick={() => debugOpen = !debugOpen}
    >
      <span class="text-primary"><Icon name="bug_report" size={20} /></span>
      Debug Log
      {#if app.logs.length > 0}
        <span class="text-[10px] px-1.5 py-0.5 rounded-full bg-surface-container-high text-on-surface-variant">
          {app.logs.length}
        </span>
      {/if}
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={debugOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if debugOpen}
      <div class="flex flex-col gap-2 mt-3 section-enter">
        <p class="text-xs text-on-surface-variant">
          Real-time log of backend events and transfers.
        </p>
        <div
          bind:this={debugEl}
          class="debug-log"
        >
          {#if app.logs.length === 0}
            <div class="text-xs text-on-surface-variant opacity-40 text-center py-4">No log entries yet</div>
          {:else}
            {#each app.logs as log}
              <div class="debug-entry" class:debug-error={log.level === "error"} class:debug-warn={log.level === "warn"} class:debug-success={log.level === "success"}>
                <span class="debug-time">{log.time}</span>
                <span class="debug-level">{log.level.toUpperCase()}</span>
                <span class="debug-text">{log.text}</span>
              </div>
            {/each}
          {/if}
        </div>
        <div class="flex gap-2">
          <Button variant="outlined" onclick={() => app.clearLogs()}>
            <Icon name="delete_sweep" size={16} />
            Clear log
          </Button>
          <Button variant="outlined" onclick={async () => {
            const text = app.logs.map(l => `[${l.time}] ${l.level.toUpperCase()} ${l.text}`).join("\n");
            await copyToClipboard(text);
            onsnackbar?.("Log copied to clipboard");
          }}>
            <Icon name="content_copy" size={16} />
            Copy log
          </Button>
        </div>
      </div>
    {/if}
  </Card>

</div>

<style>
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

  .debug-log {
    max-height: 300px;
    overflow-y: auto;
    border-radius: 8px;
    background: var(--md-sys-color-surface-container-lowest);
    border: 1px solid var(--md-sys-color-outline-variant);
    padding: 4px 0;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }
  .debug-entry {
    display: flex;
    gap: 6px;
    padding: 2px 8px;
    font-family: "Roboto Mono", monospace;
    font-size: 10px;
    line-height: 1.5;
    align-items: baseline;
  }
  .debug-entry:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 4%, transparent);
  }
  .debug-time {
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
    flex-shrink: 0;
    white-space: nowrap;
  }
  .debug-level {
    flex-shrink: 0;
    width: 48px;
    font-weight: 600;
    color: var(--md-sys-color-on-surface-variant);
  }
  .debug-error .debug-level { color: var(--md-sys-color-error); }
  .debug-warn .debug-level { color: var(--md-sys-color-tertiary); }
  .debug-success .debug-level { color: #4caf50; }
  .debug-text {
    color: var(--md-sys-color-on-surface);
    white-space: pre-wrap;
    word-break: break-all;
    min-width: 0;
  }
  .debug-error .debug-text { color: var(--md-sys-color-error); }
</style>
