<!--
  Settings page — theme, receive defaults, about & status
-->
<script lang="ts">
  import Card from "$lib/ui/Card.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Switch from "$lib/ui/Switch.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Slider from "$lib/ui/Slider.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { pickSaveFolder, copyToClipboard, setMica, getDeviceIdentity, setDeviceAlias } from "$lib/api/bridge";
  import { playReceiveSound } from "$lib/utils/notification-sound";
  import { getThemeState } from "$lib/theme/theme-store.svelte";
  import { VARIANT_INFO, PRESET_COLORS, type SchemeVariant } from "$lib/theme/m3-color";

  interface Props {
    appVersion: string;
    onsnackbar?: (msg: string) => void;
  }

  let { appVersion, onsnackbar }: Props = $props();

  const app = getAppState();
  const theme = getThemeState();

  let deviceOpen = $state(false);
  let themeOpen = $state(true);
  let receiveOpen = $state(false);
  let aboutOpen = $state(false);
  let debugOpen = $state(false);
  let debugEl: HTMLDivElement | undefined = $state();

  // Device identity
  let deviceAlias = $state("");
  let deviceId = $state("");
  let aliasEditing = $state(false);

  import { onMount } from "svelte";
  onMount(async () => {
    try {
      const identity = await getDeviceIdentity();
      deviceAlias = identity.alias;
      deviceId = identity.id;
    } catch {}
  });

  async function saveAlias() {
    const trimmed = deviceAlias.trim();
    if (!trimmed) return;
    try {
      await setDeviceAlias(trimmed);
      deviceAlias = trimmed;
      aliasEditing = false;
      onsnackbar?.("Device name updated — restart to broadcast new name");
    } catch {
      onsnackbar?.("Failed to update device name");
    }
  }

  // Hex input
  let hexInput = $state(theme.seedColor.replace("#", ""));
  $effect(() => { hexInput = theme.seedColor.replace("#", ""); });

  function onHexInput(e: Event) {
    const val = (e.target as HTMLInputElement).value.replace(/[^0-9a-fA-F]/g, "").slice(0, 6);
    hexInput = val;
    if (val.length === 6) {
      theme.seedColor = "#" + val;
      theme.persist();
    }
  }

  function setSeedColor(color: string) {
    theme.seedColor = color;
    theme.persist();
  }

  function setVariant(v: SchemeVariant) {
    theme.variant = v;
    theme.persist();
  }

  function disableMica() {
    theme.mica = false;
    setMica(false, 0);
    document.documentElement.style.background = "";
    document.body.classList.remove("mica-active");
  }

  function toggleDark(checked: boolean) {
    theme.isDark = checked;
    // Force mica off when switching to light mode
    if (!checked && theme.mica) {
      disableMica();
    }
    theme.persist();
  }

  function toggleMica(checked: boolean) {
    // Never allow mica in light mode
    if (checked && !theme.isDark) return;
    theme.mica = checked;
    theme.persist();
    if (checked) {
      setMica(true, theme.micaOpacity);
      document.documentElement.style.background = "transparent";
      document.body.classList.add("mica-active");
    } else {
      disableMica();
    }
  }

  let opacityTimer: ReturnType<typeof setTimeout>;
  function onOpacityChange(val: number) {
    theme.micaOpacity = val;
    clearTimeout(opacityTimer);
    opacityTimer = setTimeout(() => theme.persist(), 500);
  }
  $effect(() => () => clearTimeout(opacityTimer));

  $effect(() => {
    if (debugOpen && app.logs.length && debugEl) {
      requestAnimationFrame(() => {
        if (debugEl) debugEl.scrollTop = debugEl.scrollHeight;
      });
    }
  });
</script>

<div class="flex flex-col gap-4">

  <!-- Device Identity -->
  <Card variant="elevated">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0 -my-0.5"
      onclick={() => deviceOpen = !deviceOpen}
    >
      <span class="text-primary"><Icon name="devices" size={20} /></span>
      This Device
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={deviceOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if deviceOpen}
      <div class="flex flex-col gap-4 mt-4 section-enter">
        <div class="flex items-center gap-3 p-4 rounded-xl bg-surface-container">
          <div class="flex items-center justify-center w-10 h-10 rounded-lg bg-primary text-on-primary">
            <Icon name="computer" size={20} />
          </div>
          <div class="flex-1 min-w-0">
            {#if aliasEditing}
              <div class="flex items-center gap-2">
                <input
                  class="flex-1 bg-transparent border-b border-primary outline-none text-sm text-on-surface py-1"
                  bind:value={deviceAlias}
                  onkeydown={(e) => { if (e.key === "Enter") saveAlias(); if (e.key === "Escape") aliasEditing = false; }}
                />
                <button
                  class="w-7 h-7 inline-flex items-center justify-center rounded-full
                         text-primary cursor-pointer bg-transparent border-none"
                  onclick={saveAlias}
                >
                  <Icon name="check" size={18} />
                </button>
              </div>
            {:else}
              <button
                class="flex items-center gap-1 text-sm text-on-surface font-medium cursor-pointer
                       bg-transparent border-none p-0 hover:text-primary"
                onclick={() => aliasEditing = true}
                title="Click to rename"
              >
                {deviceAlias || "Loading..."}
                <Icon name="edit" size={14} />
              </button>
            {/if}
            <div class="text-xs text-on-surface-variant font-mono mt-1 truncate opacity-50">{deviceId}</div>
          </div>
        </div>

        <div class="text-xs text-on-surface-variant">
          Other devices on your network will see this name. Changes take effect after restart.
        </div>
      </div>
    {/if}
  </Card>

  <!-- Theme -->
  <Card variant="elevated">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0 -my-0.5"
      onclick={() => themeOpen = !themeOpen}
    >
      <span class="text-primary"><Icon name="palette" size={20} /></span>
      Theme
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={themeOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if themeOpen}
      <div class="flex flex-col gap-6 mt-4 section-enter">

        <!-- Dark / Light toggle -->
        <div class="theme-row" class:opacity-40={theme.mica}>
          <Icon name="light_mode" size={20} />
          <Switch checked={theme.isDark} disabled={theme.mica} onchange={toggleDark} />
          <Icon name="dark_mode" size={20} />
          <span class="text-xs text-on-surface-variant">
            {theme.mica ? "Dark mode (required for transparency)" : theme.isDark ? "Dark mode" : "Light mode"}
          </span>
        </div>

        <!-- Mica / transparent background (Windows 11) -->
        <div class="flex items-center justify-between py-1" class:opacity-40={!theme.isDark}>
          <div>
            <div class="text-sm text-on-surface">Transparency</div>
            <div class="text-xs text-on-surface-variant">{!theme.isDark ? "Requires dark mode" : "Blurred translucent background"}</div>
          </div>
          <Switch checked={theme.mica} disabled={!theme.isDark} onchange={toggleMica} />
        </div>

        {#if theme.mica}
          <div class="opacity-slider">
            <div class="opacity-labels">
              <span>Clear</span>
              <span>{theme.micaOpacity}%</span>
              <span>Opaque</span>
            </div>
            <Slider
              min={0}
              max={100}
              step={1}
              bind:value={theme.micaOpacity}
              oninput={onOpacityChange}
            />
          </div>
        {/if}

        <!-- Seed Color -->
        <section>
          <h3 class="section-label">Seed color</h3>
          <div class="seed-row">
            <input
              type="color"
              value={theme.seedColor}
              oninput={(e) => setSeedColor((e.target as HTMLInputElement).value)}
              class="color-picker"
            />
            <div class="hex-input-wrap">
              <span class="hex-hash">#</span>
              <input
                value={hexInput}
                oninput={onHexInput}
                maxlength="6"
                class="hex-input"
              />
            </div>
          </div>
          <div class="preset-grid">
            {#each PRESET_COLORS as color (color)}
              <button
                onclick={() => setSeedColor(color)}
                class="preset-swatch"
                class:preset-active={theme.seedColor.toLowerCase() === color.toLowerCase()}
                style="--swatch-color: {color};"
                title={color}
              ></button>
            {/each}
          </div>
        </section>

        <!-- Scheme Variant -->
        <section>
          <h3 class="section-label">Scheme variant</h3>
          <div class="variant-grid">
            {#each VARIANT_INFO as v (v.id)}
              <button
                onclick={() => setVariant(v.id)}
                class="variant-card"
                class:variant-active={theme.variant === v.id}
              >
                <div class="variant-name">{v.name}</div>
                <div class="variant-desc">{v.desc}</div>
              </button>
            {/each}
          </div>
        </section>

      </div>
    {/if}
  </Card>

  <!-- Receive defaults -->
  <Card variant="elevated">
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
            <Button variant="elevated" full onclick={async () => {
              const f = await pickSaveFolder();
              if (f) app.updateReceiveOption("outFolder", f);
            }}>
              <Icon name="create_new_folder" size={18} />
              Set save folder
            </Button>
          {/if}
        </div>

        <div class="text-xs text-on-surface-variant mt-1">
          Files with the same name are automatically renamed
        </div>
      </div>
    {/if}
  </Card>

  <!-- Notifications -->
  <Card variant="elevated">
    <div class="flex items-center gap-2 text-on-surface text-sm font-medium">
      <span class="text-primary"><Icon name="notifications" size={20} /></span>
      Notifications
    </div>
    <div class="flex items-center justify-between py-1 mt-3">
      <div>
        <div class="text-sm text-on-surface">Notification sounds</div>
        <div class="text-xs text-on-surface-variant">Play sound when files or messages arrive</div>
      </div>
      <Switch checked={app.notificationsEnabled} onchange={(v) => { app.setNotifications(v); if (v) playReceiveSound(); }} />
    </div>
    <div class="text-xs text-on-surface-variant mt-2 opacity-60">
      Closing the window minimizes to system tray. Right-click the tray icon for options.
    </div>
  </Card>

  <!-- Hotkeys -->
  <Card variant="elevated">
    <div class="flex items-center gap-2 text-on-surface text-sm font-medium">
      <span class="text-primary"><Icon name="keyboard" size={20} /></span>
      Global Hotkeys
    </div>
    <div class="flex items-center justify-between py-1 mt-3">
      <div>
        <div class="text-sm text-on-surface">Enable hotkeys</div>
        <div class="text-xs text-on-surface-variant">System-wide shortcuts even when minimized</div>
      </div>
      <Switch checked={app.hotkeys.enabled} onchange={(v) => app.updateHotkeys({ enabled: v })} />
    </div>
    {#if app.hotkeys.enabled}
      <div class="mt-3 flex items-center gap-3">
        <div class="flex-1">
          <TextField
            label="Quick send shortcut"
            value={app.hotkeys.quickSend}
            placeholder="e.g. F3"
            oninput={(e) => {
              const val = (e.target as HTMLInputElement).value.toUpperCase();
              app.updateHotkeys({ quickSend: val });
            }}
          />
        </div>
      </div>
      <div class="text-xs text-on-surface-variant mt-1 opacity-70">
        Press this key anywhere to open file picker and attach to current chat
      </div>
    {/if}
  </Card>

  <!-- About -->
  <Card variant="elevated">
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
          <div class="flex items-center justify-center w-10 h-10 rounded-lg bg-primary text-on-primary">
            <Icon name="swap_horiz" size={20} />
          </div>
          <div class="flex-1">
            <div class="text-sm text-on-surface font-medium">LanDrop v{appVersion}</div>
            <div class="text-xs text-on-surface-variant font-mono">{app.localIp}</div>
          </div>
        </div>

        <div class="text-sm text-on-surface-variant leading-relaxed">
          <p class="font-medium text-on-surface mb-1">How it works</p>
          <ol class="list-decimal list-inside flex flex-col gap-1">
            <li>Install LanDrop on your devices</li>
            <li>Devices on the same network are discovered automatically</li>
            <li>Drop files or type messages — transferred instantly</li>
            <li>No internet, no passwords — everything stays on your LAN</li>
          </ol>
        </div>
      </div>
    {/if}
  </Card>

  <!-- Debug Log -->
  <Card variant="elevated">
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
          <Button variant="elevated" onclick={() => app.clearLogs()}>
            <Icon name="delete_sweep" size={16} />
            Clear log
          </Button>
          <Button variant="elevated" onclick={async () => {
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
    display: flex;
    align-items: center;
    justify-content: center;
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

  /* ── Theme panel ── */

  .theme-row {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }

  .section-label {
    font-size: 11px;
    font-weight: 500;
    color: var(--md-sys-color-on-surface-variant);
    margin: 0 0 8px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .seed-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
  }

  .color-picker {
    width: 48px;
    height: 48px;
    border-radius: 12px;
    border: 1px solid var(--md-sys-color-outline-variant);
    cursor: pointer;
    padding: 0;
    background: none;
  }
  .color-picker::-webkit-color-swatch-wrapper { padding: 0; }
  .color-picker::-webkit-color-swatch { border: none; border-radius: 12px; }

  .hex-input-wrap {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    border-radius: 8px;
    background: var(--md-sys-color-surface-container-high);
    border: 1px solid var(--md-sys-color-outline-variant);
  }
  .hex-hash {
    font-size: 13px;
    font-weight: 500;
    color: var(--md-sys-color-on-surface-variant);
  }
  .hex-input {
    width: 72px;
    background: transparent;
    border: none;
    outline: none;
    color: var(--md-sys-color-on-surface);
    font-size: 14px;
    font-family: "Roboto Mono", monospace;
    font-weight: 600;
    text-transform: uppercase;
  }

  .preset-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
  }
  .preset-swatch {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 2px solid transparent;
    background: var(--swatch-color);
    cursor: pointer;
    transition: transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial), border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .preset-swatch:hover {
    transform: scale(1.15);
  }
  .preset-active {
    border-color: var(--md-sys-color-on-surface);
    transform: scale(1.15);
  }

  .variant-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .variant-card {
    padding: 10px;
    border-radius: 12px;
    text-align: left;
    cursor: pointer;
    border: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects), border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .variant-card:hover {
    background: var(--md-sys-color-surface-container-highest);
  }
  .variant-active {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    border-color: var(--md-sys-color-primary);
    border-width: 2px;
  }
  .variant-name {
    font-size: 12px;
    font-weight: 500;
  }
  .variant-desc {
    font-size: 10px;
    opacity: 0.7;
    margin-top: 2px;
  }

  .opacity-slider {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .opacity-labels {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.7;
  }

  /* ── Debug log ── */

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
