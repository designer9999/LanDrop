<!--
  Root Application Component — LanDrop
  M3 Expressive dark theme — elevated design
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { getThemeState } from "$lib/theme/theme-store.svelte";
  import { applyThemeToDOM } from "$lib/theme/apply-theme";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { MessageAttachment } from "$lib/state/app-state.svelte";
  import { getStatus, startLanService, lanSendText, lanSendFiles, onLanLog, onLanPeerDiscovered, onLanPeerLost, onLanTextReceived, onLanFilesReceived, onTransferProgress, windowMinimize, windowToggleMaximize, windowClose, windowStartDrag, windowShow, setMica, setDefaultOutFolder, registerShortcut, unregisterShortcut, getFileInfo, getExplorerSelection, getClipboardFiles } from "$lib/api/bridge";
  import type { TransferProgress } from "$lib/api/bridge";
  import { isImage as fileIsImage, fileSizeStr } from "$lib/utils/file-utils";
  import { playReceiveSound } from "$lib/utils/notification-sound";

  import Icon from "$lib/ui/Icon.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import Snackbar from "$lib/ui/Snackbar.svelte";
  import PeerBar from "./features/peers/PeerBar.svelte";
  import DeviceSettingsDialog from "./features/peers/DeviceSettingsDialog.svelte";
  import TransferPage from "./features/transfer/TransferPage.svelte";
  import SettingsPage from "./features/settings/SettingsPage.svelte";

  const theme = getThemeState();
  const app = getAppState();

  let snackbarMsg = $state("");
  let snackbarVisible = $state(false);
  let appVersion = $state("1.0.0");
  let transferProgress = $state<TransferProgress | null>(null);

  let deviceDialogOpen = $state(false);
  let editingDevice = $state<import("$lib/state/app-state.svelte").DiscoveredDevice | null>(null);

  function showSnackbar(msg: string) {
    snackbarMsg = msg;
    snackbarVisible = true;
  }

  function openDeviceSettings(id: string) {
    editingDevice = app.devices.find(d => d.id === id) ?? null;
    deviceDialogOpen = true;
  }

  $effect(() => {
    applyThemeToDOM(theme.tokens);
  });

  // Sync default out folder to backend when settings change
  $effect(() => {
    const folder = app.receiveOptions.outFolder ?? "";
    setDefaultOutFolder(folder);
  });

  onMount(async () => {
    const status = await getStatus();
    app.localIp = status.local_ip ?? "unknown";
    if (status.app_version) appVersion = status.app_version;

    // Restore mica if enabled
    if (theme.mica) {
      setMica(true, theme.micaOpacity);
      document.documentElement.style.background = "transparent";
      document.body.classList.add("mica-active");
    }

    // Start mDNS discovery — zero config, no passwords
    await startLanService();

    const unlisteners = await Promise.all([
      onLanLog((level, text) => {
        const mapped = level === "success" ? "success" : level === "error" ? "error" : level === "warn" ? "warn" : "info";
        app.addLog(mapped as "info" | "warn" | "error" | "success", text);
      }),
      onLanPeerDiscovered((peer) => {
        app.upsertDevice(peer);
        app.addLog("success", `Device discovered: ${peer.alias} (${peer.ip})`);
      }),
      onLanPeerLost((peerId) => {
        app.markDeviceOffline(peerId);
        const device = app.devices.find(d => d.id === peerId);
        app.addLog("warn", `Device offline: ${device?.alias ?? peerId}`);
      }),
      onLanTextReceived((peerId, text) => {
        // Auto-select device if we receive from it and have no active
        if (!app.activeDeviceId) app.setActiveDevice(peerId);
        app.addMessage({ peerId, direction: "received", text });
        app.addActivity({ peerId, direction: "received", type: "text", items: [], success: true });
        if (app.notificationsEnabled) playReceiveSound();
      }),
      onLanFilesReceived((peerId, files, details) => {
        if (!app.activeDeviceId) app.setActiveDevice(peerId);
        app.addActivity({ peerId, direction: "received", type: "files", items: files, success: true, outFolder: app.effectiveOutFolder });
        if (details.length > 0) {
          // Group files that came from a folder
          const folderFiles = new Map<string, typeof details>();
          const looseFiles: typeof details = [];
          for (const f of details) {
            const slashIdx = f.name.indexOf("/");
            if (slashIdx > 0) {
              const folder = f.name.substring(0, slashIdx);
              if (!folderFiles.has(folder)) folderFiles.set(folder, []);
              folderFiles.get(folder)!.push(f);
            } else {
              looseFiles.push(f);
            }
          }

          const attachments: MessageAttachment[] = [];
          for (const [folder, folderDetails] of folderFiles) {
            const totalSize = folderDetails.reduce((sum, f) => sum + f.size, 0);
            const folderPath = folderDetails[0].path.replace(/[\\/][^\\/]+$/, "");
            attachments.push({
              name: folder, path: folderPath, size: fileSizeStr(totalSize),
              type: "folder" as const, fileCount: folderDetails.length,
            });
          }
          for (const f of looseFiles) {
            attachments.push({
              name: f.name, path: f.path, size: fileSizeStr(f.size),
              type: fileIsImage(f.name) ? "image" as const : "file" as const,
            });
          }
          app.addMessage({ peerId, direction: "received", text: "", attachments });
        }
        if (app.notificationsEnabled) playReceiveSound();
      }),
      onTransferProgress((progress) => {
        transferProgress = progress.phase === "done" ? null : progress;
      }),
    ]);

    // Register global hotkeys
    await setupHotkeys();

    return () => { unlisteners.forEach(fn => fn()); };
  });

  // ── Global hotkeys ──
  async function quickSendHandler() {
    let paths = await getExplorerSelection().catch(() => [] as string[]);
    if (paths.length === 0) {
      paths = await getClipboardFiles().catch(() => [] as string[]);
    }
    if (paths.length === 0) return;
    for (const path of paths) {
      const info = await getFileInfo(path);
      app.addFile(path, info);
    }
    await windowShow();
  }

  async function setupHotkeys() {
    if (!app.hotkeys.enabled) return;
    try {
      await registerShortcut(app.hotkeys.quickSend, quickSendHandler);
    } catch (e) {
      app.addLog("warn", `Hotkey registration failed: ${e}`);
    }
  }

  $effect(() => {
    const { quickSend, enabled } = app.hotkeys;
    if (enabled) {
      registerShortcut(quickSend, quickSendHandler).catch(() => {});
    } else {
      unregisterShortcut(quickSend).catch(() => {});
    }
  });

  async function handleSendFiles() {
    if (!app.hasFiles || app.transferActive) return;
    const device = app.activeDevice;
    if (!device) { showSnackbar("No device selected"); return; }
    if (!device.online) { showSnackbar("Device is offline"); return; }

    const filesCopy = [...app.files];
    app.transferActive = true;
    try {
      const sent = await lanSendFiles(device.id, app.filePaths);
      if (sent) {
        const names = filesCopy.map(f => f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file");
        app.addActivity({ peerId: device.id, direction: "sent", type: "files", items: names, success: true });
        const attachments: MessageAttachment[] = filesCopy.map(f => ({
          name: f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file",
          path: f.path, size: f.info?.size ?? "",
          type: fileIsImage(f.info?.name ?? f.path) ? "image" as const : "file" as const,
        }));
        app.addMessage({ peerId: device.id, direction: "sent", text: "", attachments });
        app.clearFiles();
      }
    } catch (e) {
      showSnackbar("Send failed — " + e);
    } finally {
      app.transferActive = false;
    }
  }

  async function handleSendText() {
    if (!app.sendTextContent.trim() || app.transferActive) return;
    const device = app.activeDevice;
    if (!device) { showSnackbar("No device selected"); return; }
    const textToSend = app.sendTextContent.trim();
    app.sendTextContent = "";
    if (!device.online) {
      app.addMessage({ peerId: device.id, direction: "sent", text: textToSend });
      showSnackbar("Device is offline — message saved locally");
      return;
    }
    try {
      const sent = await lanSendText(device.id, textToSend);
      if (sent) {
        app.addMessage({ peerId: device.id, direction: "sent", text: textToSend });
        app.addActivity({ peerId: device.id, direction: "sent", type: "text", items: [], success: true });
      }
    } catch {
      showSnackbar("Text send failed — device unreachable");
    }
  }

  const progressPct = $derived.by(() => {
    if (!transferProgress?.total_bytes || transferProgress.total_bytes === 0) return null;
    const done = transferProgress.sent_bytes ?? transferProgress.received_bytes ?? 0;
    return Math.round((done / transferProgress.total_bytes) * 100);
  });

  function handleTitlebarMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, input, a, [role='button'], .peer-chip")) return;
    if (e.detail === 2) {
      windowToggleMaximize();
    } else {
      windowStartDrag();
    }
  }
</script>

<div class="app-shell" class:mica-on={theme.mica} style:--mica-opacity={theme.mica ? (0.05 + theme.micaOpacity * 0.75 / 100) : 1}>
  <!-- Custom title bar -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="titlebar" onmousedown={handleTitlebarMouseDown}>
    <div class="titlebar-content">
      {#if app.activeView === "transfer"}
        <div class="peer-strip">
          <PeerBar onedit={openDeviceSettings} />
        </div>
      {:else}
        <div class="flex items-center gap-2 flex-1 min-w-0">
          <Icon name="settings" size={18} />
          <span class="text-sm font-medium">Settings</span>
        </div>
      {/if}

      <IconButton
        title={app.activeView === "settings" ? "Back" : "Settings"}
        onclick={() => app.activeView = app.activeView === "settings" ? "transfer" : "settings"}
      >
        <Icon name={app.activeView === "settings" ? "close" : "tune"} size={20} />
      </IconButton>
    </div>

    <div class="titlebar-controls">
      <IconButton title="Minimize" onclick={windowMinimize}>
        <Icon name="remove" size={20} />
      </IconButton>
      <IconButton title="Close" onclick={windowClose}>
        <Icon name="close" size={20} />
      </IconButton>
    </div>
  </div>

  <!-- Content area -->
  <main class="content-area">
    {#if app.activeView === "transfer"}
      <TransferPage onsnackbar={showSnackbar} onsend={handleSendFiles} onsendtext={handleSendText} />
    {:else}
      <div class="settings-scroll">
        <SettingsPage {appVersion} onsnackbar={showSnackbar} />
      </div>
    {/if}
  </main>

  <!-- Transfer Progress -->
  {#if transferProgress}
    <div class="progress-bar">
      <div class="progress-bar-fill" style="width: {progressPct ?? 0}%"></div>
      <div class="progress-bar-content">
        <Icon name={transferProgress.direction === "send" ? "upload" : "download"} size={14} />
        <span class="truncate flex-1">
          {transferProgress.direction === "send" ? "Sending" : "Receiving"}
          {#if transferProgress.current_file}
            {transferProgress.current_file.split("/").pop()}
          {/if}
        </span>
        {#if progressPct !== null}
          <span class="progress-pct">{progressPct}%</span>
        {/if}
      </div>
    </div>
  {/if}

</div>

<DeviceSettingsDialog
  bind:open={deviceDialogOpen}
  device={editingDevice}
  onclose={() => { deviceDialogOpen = false; editingDevice = null; }}
/>

<Snackbar message={snackbarMsg} bind:visible={snackbarVisible} />

<style>
  .app-shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    overflow: hidden;
    user-select: none;
  }

  /* ── Title Bar ── */
  .titlebar {
    display: flex;
    align-items: stretch;
    background: var(--md-sys-color-surface);
    border-bottom: 1px solid color-mix(in srgb, var(--md-sys-color-outline-variant) 50%, transparent);
    flex-shrink: 0;
  }
  .titlebar-content {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 4px 8px 8px;
    flex: 1;
    min-width: 0;
    min-height: 48px;
  }
  .titlebar-controls {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    gap: 2px;
    padding-right: 4px;
  }
  .peer-strip {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* ── Content ── */
  .content-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .settings-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }

  /* ── Progress Bar ── */
  .progress-bar {
    position: relative;
    overflow: hidden;
    border-top: 1px solid var(--md-sys-color-outline-variant);
  }
  .progress-bar-fill {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--md-sys-color-primary) 15%, transparent);
    transition: width var(--md-spring-default-effects-dur) var(--md-spring-default-effects);
  }
  .progress-bar-content {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 16px;
    font-size: 12px;
    color: var(--md-sys-color-primary);
  }
  .progress-pct {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    font-size: 11px;
  }


  /* ── Mica mode — transparent backgrounds ── */
  :global(body.mica-active) {
    background: transparent !important;
  }
  .app-shell.mica-on {
    background: rgba(0, 0, 0, var(--mica-opacity, 0.7));
  }
  .mica-on .titlebar {
    background: transparent;
  }
  .mica-on :global(.composer) {
    background: transparent !important;
  }
  .mica-on :global(.composer-box) {
    background: var(--md-sys-color-surface) !important;
  }
  .mica-on :global(.chat-toolbar) {
    background: transparent;
  }
  .mica-on .settings-scroll {
    background: transparent;
  }
  .mica-on :global(.bg-surface-container-low) {
    background: rgba(255, 255, 255, 0.06) !important;
    backdrop-filter: blur(4px);
  }
  .mica-on :global(.shadow-level1) {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3) !important;
  }
</style>
