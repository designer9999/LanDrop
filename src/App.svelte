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
  import { getStatus, startLanService, lanSendText, lanSendFiles, onLanLog, onLanPeerDiscovered, onLanPeerLost, onLanTextReceived, onLanFilesReceived, onTransferProgress, windowMinimize, windowToggleMaximize, windowClose, windowStartDrag, windowShow, setMica, setDefaultOutFolder, setPeerOutFolder, setReceiveSortByDate, getReceiveFolderSettings, registerShortcut, unregisterShortcut, getFileInfo, getExplorerSelection, getClipboardFiles } from "$lib/api/bridge";
  import type { TransferProgress } from "$lib/api/bridge";
  import { loadPersistedAppState, savePersistedAppState } from "$lib/persistence/app-store";
  import { isImage as fileIsImage, isVideo as fileIsVideo, fileSizeStr } from "$lib/utils/file-utils";
  import { sendNativeNotification } from "$lib/utils/native-notifications";
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
  let persistedStateReady = $state(false);
  let receiveSettingsReady = $state(false);
  let persistedPeerFolders = $state<Record<string, string>>({});
  let transferRateBps = $state<number | null>(null);
  let transferEtaSeconds = $state<number | null>(null);
  let lastProgressSample = $state<{ bytes: number; at: number; direction: "send" | "receive" } | null>(null);

  function showSnackbar(msg: string) {
    snackbarMsg = msg;
    snackbarVisible = true;
  }

  function openDeviceSettings(id: string) {
    editingDevice = app.devices.find(d => d.id === id) ?? null;
    deviceDialogOpen = true;
  }

  function getReceivedFolderPath(filePath: string, relativeName: string): string {
    let folderPath = filePath;
    const segmentCount = relativeName.split("/").filter(Boolean).length;
    for (let i = 1; i < segmentCount; i += 1) {
      folderPath = folderPath.replace(/[\\/][^\\/]+$/, "");
    }
    return folderPath;
  }

  function joinReceivePath(baseFolder: string, name: string): string {
    const trimmedBase = baseFolder.replace(/[\\/]+$/, "");
    if (!trimmedBase) return name;
    const separator = trimmedBase.includes("\\") ? "\\" : "/";
    const normalizedName = name
      .replace(/^[/\\]+/, "")
      .replace(/[\\/]+/g, separator);
    return `${trimmedBase}${separator}${normalizedName}`;
  }

  function getConfiguredOutFolder(peerId: string): string {
    return app.devices.find((device) => device.id === peerId)?.outFolder
      ?? app.receiveOptions.outFolder
      ?? "";
  }

  function getAttachmentCandidates(attachment: MessageAttachment): Array<{ name: string; path: string }> {
    return [
      { name: attachment.name, path: attachment.path },
      ...(attachment.children ?? []).map((child) => ({ name: child.name, path: child.path })),
    ];
  }

  async function repairStoredMessagePaths() {
    let repairedCount = 0;

    for (const message of app.messages) {
      if (message.direction !== "received" || !message.attachments?.length) continue;

      const outFolder = getConfiguredOutFolder(message.peerId).trim();
      if (!outFolder) continue;

      for (const attachment of message.attachments) {
        for (const candidate of getAttachmentCandidates(attachment)) {
          if (await getFileInfo(candidate.path)) continue;

          const repairedPath = joinReceivePath(outFolder, candidate.name);
          if (!await getFileInfo(repairedPath)) continue;

          app.updateAttachmentPath(message.id, candidate.path, repairedPath);
          repairedCount += 1;
        }
      }
    }

    if (repairedCount > 0) {
      showSnackbar(`Re-linked ${repairedCount} saved item${repairedCount === 1 ? "" : "s"} to your current folder`);
    }
  }

  $effect(() => {
    applyThemeToDOM(theme.tokens);
  });

  let persistedStateSaveTimer: ReturnType<typeof setTimeout>;
  let previousPersistedState = "";
  $effect(() => {
    if (!persistedStateReady) return;
    const snapshot = app.exportPersistedState();
    const serialized = JSON.stringify(snapshot);
    if (serialized === previousPersistedState) return;

    previousPersistedState = serialized;
    clearTimeout(persistedStateSaveTimer);
    persistedStateSaveTimer = setTimeout(() => {
      savePersistedAppState(snapshot).catch(() => {});
    }, 200);
  });
  $effect(() => () => clearTimeout(persistedStateSaveTimer));

  // Sync default out folder to backend when settings change
  $effect(() => {
    if (!receiveSettingsReady) return;
    const folder = app.receiveOptions.outFolder ?? "";
    setDefaultOutFolder(folder);
  });

  $effect(() => {
    if (!receiveSettingsReady) return;
    const sortByDate = app.receiveOptions.sortByDate ?? false;
    setReceiveSortByDate(sortByDate);
  });

  let previousPeerFolderState = "";
  $effect(() => {
    if (!receiveSettingsReady) return;
    const peerFolders = app.devices
      .map((device) => `${device.id}:${device.outFolder ?? ""}`)
      .sort()
      .join("|");

    if (peerFolders === previousPeerFolderState) return;
    previousPeerFolderState = peerFolders;

    for (const device of app.devices) {
      setPeerOutFolder(device.id, device.outFolder ?? "");
    }
  });

  onMount(() => {
    let unlisteners: Array<() => void> = [];

    (async () => {
      const persistedState = await loadPersistedAppState().catch(() => null);
      if (persistedState) {
        app.hydratePersistedState(persistedState);
      } else {
        const legacyState = app.loadLegacyPersistedState();
        if (legacyState) {
          app.hydratePersistedState(legacyState);
          await savePersistedAppState(legacyState).catch(() => {});
          app.clearLegacyPersistedState();
        }
      }
      previousPersistedState = JSON.stringify(app.exportPersistedState());
      persistedStateReady = true;

      const status = await getStatus();
      app.localIp = status.local_ip ?? "unknown";
      if (status.app_version) appVersion = status.app_version;

      try {
        const savedFolders = await getReceiveFolderSettings();
        persistedPeerFolders = savedFolders.peer_folders ?? {};

        const savedDefaultFolder = savedFolders.default_out_folder?.trim() ?? "";
        if (savedDefaultFolder && savedDefaultFolder !== (app.receiveOptions.outFolder?.trim() ?? "")) {
          app.updateReceiveOption("outFolder", savedDefaultFolder || undefined);
        }
        if ((savedFolders.sort_by_date ?? false) !== (app.receiveOptions.sortByDate ?? false)) {
          app.updateReceiveOption("sortByDate", savedFolders.sort_by_date ?? false);
        }

        for (const [peerId, folder] of Object.entries(persistedPeerFolders)) {
          const existingDevice = app.devices.find((device) => device.id === peerId);
          if (existingDevice && (existingDevice.outFolder ?? "") !== folder) {
            app.updateDeviceSettings(peerId, { outFolder: folder || undefined });
          }
        }
      } catch {}
      receiveSettingsReady = true;

      // Restore mica if enabled
      if (theme.mica) {
        setMica(true);
        document.documentElement.style.background = "transparent";
        document.body.classList.add("mica-active");
      }

      // Start mDNS discovery — zero config, no passwords
      await startLanService();
      await repairStoredMessagePaths();

      unlisteners = await Promise.all([
        onLanLog((level, text) => {
          const mapped = level === "success" ? "success" : level === "error" ? "error" : level === "warn" ? "warn" : "info";
          app.addLog(mapped as "info" | "warn" | "error" | "success", text);
        }),
        onLanPeerDiscovered((peer) => {
          app.upsertDevice(peer);
          const savedOutFolder = persistedPeerFolders[peer.id];
          if (savedOutFolder && app.devices.find((device) => device.id === peer.id)?.outFolder !== savedOutFolder) {
            app.updateDeviceSettings(peer.id, { outFolder: savedOutFolder });
          }
          app.addLog("success", `Device discovered: ${peer.alias} (${peer.ip})`);
        }),
        onLanPeerLost((peerId) => {
          app.markDeviceOffline(peerId);
          const device = app.devices.find(d => d.id === peerId);
          app.addLog("warn", `Device offline: ${device?.alias ?? peerId}`);
        }),
        onLanTextReceived((peerId, text) => {
          if (!app.activeDeviceId) app.setActiveDevice(peerId);
          app.addMessage({ peerId, direction: "received", text });
          app.addActivity({ peerId, direction: "received", type: "text", items: [], success: true });
          if (app.notificationsEnabled) playReceiveSound();
          if (app.notificationsEnabled && !document.hasFocus()) {
            sendNativeNotification(
              app.devices.find((device) => device.id === peerId)?.alias ?? "LanDrop",
              text || "New message received"
            ).catch(() => {});
          }
          if (app.popOnReceive) windowShow();
        }),
        onLanFilesReceived((peerId, files, details) => {
          if (!app.activeDeviceId) app.setActiveDevice(peerId);
          app.addActivity({ peerId, direction: "received", type: "files", items: files, success: true, outFolder: app.effectiveOutFolder });
          if (details.length > 0) {
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
              const folderPath = getReceivedFolderPath(folderDetails[0].path, folderDetails[0].name);
              const children = folderDetails.map((detail) => ({
                name: detail.name,
                path: detail.path,
                size: fileSizeStr(detail.size),
                type: fileIsImage(detail.name) ? "image" as const : fileIsVideo(detail.name) ? "video" as const : "file" as const,
              }));
              attachments.push({
                name: folder, path: folderPath, size: fileSizeStr(totalSize),
                type: "folder" as const, fileCount: folderDetails.length, children,
              });
            }
            for (const f of looseFiles) {
              attachments.push({
                name: f.name, path: f.path, size: fileSizeStr(f.size),
                type: fileIsImage(f.name) ? "image" as const : fileIsVideo(f.name) ? "video" as const : "file" as const,
              });
            }
            app.addMessage({ peerId, direction: "received", text: "", attachments });
          }
          if (app.notificationsEnabled) playReceiveSound();
          if (app.notificationsEnabled && !document.hasFocus()) {
            const peerAlias = app.devices.find((device) => device.id === peerId)?.alias ?? "LanDrop";
            const body = files.length === 1 ? `Received ${files[0]}` : `Received ${files.length} items`;
            sendNativeNotification(peerAlias, body).catch(() => {});
          }
          if (app.popOnReceive) windowShow();
        }),
        onTransferProgress((progress) => {
          const completedBytes = progress.sent_bytes ?? progress.received_bytes ?? 0;
          const totalBytes = progress.total_bytes ?? 0;
          if (progress.phase === "start") {
            lastProgressSample = completedBytes > 0 ? { bytes: completedBytes, at: Date.now(), direction: progress.direction } : null;
            transferRateBps = null;
            transferEtaSeconds = null;
          } else if (progress.phase === "transferring" && totalBytes > 0) {
            const now = Date.now();
            if (lastProgressSample && lastProgressSample.direction === progress.direction) {
              const elapsedMs = now - lastProgressSample.at;
              const byteDelta = completedBytes - lastProgressSample.bytes;
              if (elapsedMs > 0 && byteDelta > 0) {
                transferRateBps = byteDelta / (elapsedMs / 1000);
                const remainingBytes = Math.max(0, totalBytes - completedBytes);
                transferEtaSeconds = transferRateBps > 0 ? remainingBytes / transferRateBps : null;
              }
            }
            lastProgressSample = { bytes: completedBytes, at: now, direction: progress.direction };
          } else if (progress.phase === "done") {
            lastProgressSample = null;
            transferRateBps = null;
            transferEtaSeconds = null;
          }
          transferProgress = progress.phase === "done" ? null : progress;
        }),
      ]);

      await setupHotkeys();
    })();

    return () => { unlisteners.forEach(fn => fn()); };
  });

  // ── Global hotkeys ──
  async function quickSendHandler() {
    let paths = await getExplorerSelection().catch(() => [] as string[]);
    if (paths.length === 0) {
      paths = await getClipboardFiles().catch(() => [] as string[]);
    }
    if (paths.length === 0) {
      // Nothing selected — just bring app to focus
      await windowShow();
      return;
    }
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

  let _prevHotkeyKey = "";
  $effect(() => {
    const { quickSend, enabled } = app.hotkeys;
    const key = `${enabled}:${quickSend}`;
    if (key === _prevHotkeyKey) return;
    // Unregister old shortcut if it was different
    if (_prevHotkeyKey) {
      const oldShortcut = _prevHotkeyKey.split(":").slice(1).join(":");
      unregisterShortcut(oldShortcut).catch(() => {});
    }
    _prevHotkeyKey = key;
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
    if (!device.online && !device.ip.trim()) { showSnackbar("Device is offline"); return; }

    const filesCopy = [...app.files];
    const pathsCopy = [...app.filePaths];
    app.transferActive = true;
    try {
      const sent = await lanSendFiles(device.id, pathsCopy, device.ip);
      if (sent) {
        const names = filesCopy.map(f => f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file");
        app.addActivity({ peerId: device.id, direction: "sent", type: "files", items: names, success: true });
        const attachments: MessageAttachment[] = filesCopy.map(f => ({
          name: f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file",
          path: f.path, size: f.info?.size ?? "",
          type: fileIsImage(f.info?.name ?? f.path) ? "image" as const : fileIsVideo(f.info?.name ?? f.path) ? "video" as const : "file" as const,
        }));
        app.addMessage({ peerId: device.id, direction: "sent", text: "", attachments });
        app.clearFiles();
      }
    } catch (e) {
      showSnackbar("Send failed — " + e);
      app.markDeviceOffline(device.id);
      app.addLog("warn", `Send failed for ${device.alias} (${device.ip || "unknown ip"}) — restarting discovery: ${e}`);
      startLanService().catch(() => {});
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
    if (!device.online && !device.ip.trim()) {
      app.addMessage({ peerId: device.id, direction: "sent", text: textToSend });
      showSnackbar("Device is offline — message saved locally");
      return;
    }
    try {
      const sent = await lanSendText(device.id, textToSend, device.ip);
      if (sent) {
        app.addMessage({ peerId: device.id, direction: "sent", text: textToSend });
        app.addActivity({ peerId: device.id, direction: "sent", type: "text", items: [], success: true });
      }
    } catch (e: any) {
      app.markDeviceOffline(device.id);
      showSnackbar(`Send failed — ${e?.message ?? e ?? "device unreachable"}`);
    }
  }

  const progressPct = $derived.by(() => {
    if (!transferProgress?.total_bytes || transferProgress.total_bytes === 0) return null;
    const done = transferProgress.sent_bytes ?? transferProgress.received_bytes ?? 0;
    return Math.round((done / transferProgress.total_bytes) * 100);
  });

  const transferRateLabel = $derived.by(() => {
    if (!transferRateBps) return "";
    return `${fileSizeStr(Math.round(transferRateBps))}/s`;
  });

  const transferEtaLabel = $derived.by(() => {
    if (!transferEtaSeconds || !isFinite(transferEtaSeconds)) return "";
    const secs = Math.max(0, Math.round(transferEtaSeconds));
    if (secs < 60) return `${secs}s left`;
    const mins = Math.floor(secs / 60);
    const rem = secs % 60;
    return `${mins}m ${rem}s left`;
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
        <div class="progress-text">
          <span class="truncate flex-1">
            {transferProgress.direction === "send" ? "Sending" : "Receiving"}
            {#if transferProgress.current_file}
              {transferProgress.current_file.split("/").pop()}
            {/if}
          </span>
          {#if transferRateLabel || transferEtaLabel}
            <span class="progress-meta">{[transferRateLabel, transferEtaLabel].filter(Boolean).join(" · ")}</span>
          {/if}
        </div>
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
  .progress-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .progress-meta {
    font-size: 10px;
    opacity: 0.75;
    white-space: nowrap;
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
