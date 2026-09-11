<!--
  Root Application Component — LanDrop
  M3 Expressive dark theme — elevated design
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { getThemeState } from "$lib/theme/theme-store.svelte";
  import { applyThemeToDOM } from "$lib/theme/apply-theme";
  import { getAppState, PEER_COLORS } from "$lib/state/app-state.svelte";
  import type { MessageAttachment } from "$lib/state/app-state.svelte";
  import {
    getStatus,
    startLanService,
    lanSendText,
    lanSendFiles,
    onLanLog,
    onLanPeerDiscovered,
    onLanPeerLost,
    onTailscaleStatus,
    onLanTextReceived,
    onLanFilesReceived,
    onTransferProgress,
    windowMinimize,
    windowToggleMaximize,
    windowClose,
    windowStartDrag,
    windowShow,
    setMica,
    getReceiveFolderSettings,
    registerShortcut,
    unregisterShortcut,
    getFileInfo,
    getExplorerSelection,
    getClipboardFiles,
    isMobile,
  } from "$lib/api/bridge";
  import type { PreparedSendPath, TransferProgress } from "$lib/api/bridge";
  import { loadPersistedAppState, savePersistedAppState } from "$lib/persistence/app-store";
  import {
    fileNameFromPath,
    isImage as fileIsImage,
    isVideo as fileIsVideo,
    fileSizeStr,
    getReceivedFolderPath,
    joinReceivePath,
    limitHistoryItems,
  } from "$lib/utils/file-utils";
  import {
    sendNativeNotification,
    onNotificationActivation,
    type NotificationTarget,
  } from "$lib/utils/native-notifications";
  import { getUpdaterState } from "$lib/state/updater-state.svelte";
  import { openNotificationPeer } from "$lib/utils/notification-routing";
  import { playReceiveSound } from "$lib/utils/notification-sound";

  import Icon from "$lib/ui/Icon.svelte";
  import Dialog from "$lib/ui/Dialog.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import Snackbar from "$lib/ui/Snackbar.svelte";
  import PeerBar from "./features/peers/PeerBar.svelte";
  import DeviceSettingsDialog from "./features/peers/DeviceSettingsDialog.svelte";
  import TransferPage from "./features/transfer/TransferPage.svelte";
  import SettingsPage from "./features/settings/SettingsPage.svelte";

  const theme = getThemeState();
  const app = getAppState();
  const updater = getUpdaterState();
  let pendingNotificationPeer = $state<string | null>(null);
  let notificationDialogOpen = $state(false);

  function openNotificationConversation(peerId: string) {
    const result = openNotificationPeer(app, peerId);
    if (result === "missing") {
      showSnackbar("This conversation is no longer in your saved history.");
    } else if (result === "busy") {
      showSnackbar("Finish the current transfer before switching conversations.");
    } else if (result === "draft") {
      pendingNotificationPeer = peerId;
      notificationDialogOpen = true;
    }
  }

  function handleNotificationActivation(target: NotificationTarget) {
    void windowShow();
    if (target.kind === "updates") {
      app.activeView = "settings";
      updater.openDetails();
      void updater.check();
      return;
    }
    openNotificationConversation(target.peerId);
  }

  let snackbarMsg = $state("");
  let snackbarVisible = $state(false);
  let appVersion = $state("1.0.0");
  let transferProgress = $state<TransferProgress | null>(null);

  let deviceDialogOpen = $state(false);
  let editingDevice = $state<import("$lib/state/app-state.svelte").DiscoveredDevice | null>(null);
  let persistedStateReady = $state(false);
  // Read-only display snapshot of Rust receive_folders.json, taken once at
  // startup; Rust remains the single owner (UI writes go through
  // set_peer_out_folder / set_default_out_folder at the moment of action).
  let startupPeerFolders: Record<string, string> = {};
  let transferRateBps = $state<number | null>(null);
  let transferEtaSeconds = $state<number | null>(null);
  let lastProgressSample = $state<{
    bytes: number;
    at: number;
    direction: "send" | "receive";
  } | null>(null);
  const MAX_HISTORY_ITEMS = 100;

  function showSnackbar(msg: string) {
    snackbarMsg = msg;
    snackbarVisible = true;
  }

  function openDeviceSettings(id: string) {
    editingDevice = app.devices.find((d) => d.id === id) ?? null;
    deviceDialogOpen = true;
  }

  function ensurePeerVisible(peerId: string) {
    if (app.devices.some((device) => device.id === peerId)) return;
    app.devices = [
      ...app.devices,
      {
        id: peerId,
        alias: `Device-${peerId.slice(0, 8)}`,
        deviceType: "desktop",
        ip: "",
        online: false,
        color: app.devices.length % PEER_COLORS.length,
      },
    ];
    app.addLog("warn", `Recovered hidden peer ${peerId.slice(0, 8)} from inbound traffic`);
  }

  function revealIncomingPeer(peerId: string) {
    ensurePeerVisible(peerId);
    // Incoming traffic must not change the recipient of a draft or interrupt history/settings.
    if (
      !app.activeDeviceId &&
      !app.sendTextContent &&
      !app.hasFiles &&
      !app.transferActive &&
      !app.messageViewAll
    ) {
      app.setActiveDevice(peerId);
    }
  }

  function getConfiguredOutFolder(peerId: string): string {
    return (
      app.devices.find((device) => device.id === peerId)?.outFolder ??
      app.receiveOptions.outFolder ??
      ""
    );
  }

  function getAttachmentCandidates(
    attachment: MessageAttachment,
  ): Array<{ name: string; path: string }> {
    return [
      { name: attachment.name, path: attachment.path },
      ...(Array.isArray(attachment.children) ? attachment.children : []).map((child) => ({
        name: child.name,
        path: child.path,
      })),
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
          if (!(await getFileInfo(repairedPath))) continue;

          app.updateAttachmentPath(message.id, candidate.path, repairedPath);
          repairedCount += 1;
        }
      }
    }

    if (repairedCount > 0) {
      showSnackbar(
        `Re-linked ${repairedCount} saved item${repairedCount === 1 ? "" : "s"} to your current folder`,
      );
    }
  }

  $effect(() => {
    applyThemeToDOM(theme.tokens);
  });

  let persistedStateSaveTimer: ReturnType<typeof setTimeout>;
  updater.prepareToExit = async () => {
    clearTimeout(persistedStateSaveTimer);
    await savePersistedAppState(app.exportPersistedState());
  };
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

  onMount(() => {
    let unlisteners: Array<() => void> = [];
    let disposed = false;
    let activeReceives = 0;

    function cleanupListeners() {
      for (const unlisten of unlisteners.splice(0)) unlisten();
    }

    void (async () => {
      const persistedState = await loadPersistedAppState().catch(() => null);
      if (disposed) return;
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

      try {
        const unlisten = await onNotificationActivation(handleNotificationActivation);
        if (disposed) {
          unlisten();
          return;
        }
        unlisteners.push(unlisten);
      } catch (error) {
        app.addLog("warn", `Notification activation unavailable: ${error}`);
      }

      const status = await getStatus();
      if (disposed) return;
      app.localIp = status.local_ip ?? "unknown";
      if (status.app_version) appVersion = status.app_version;

      // Hydrate the receive-folder display state from Rust — the single owner.
      try {
        const savedFolders = await getReceiveFolderSettings();
        if (disposed) return;
        startupPeerFolders = savedFolders.peer_folders ?? {};

        const savedDefaultFolder = savedFolders.default_out_folder?.trim() ?? "";
        app.updateReceiveOption("outFolder", savedDefaultFolder || undefined);
        app.updateReceiveOption("sortByDate", savedFolders.sort_by_date ?? false);

        for (const [peerId, folder] of Object.entries(startupPeerFolders)) {
          const existingDevice = app.devices.find((device) => device.id === peerId);
          if (existingDevice && (existingDevice.outFolder ?? "") !== folder) {
            app.updateDeviceSettings(peerId, { outFolder: folder || undefined });
          }
        }
      } catch {}

      // Restore mica if enabled
      if (theme.mica) {
        setMica(true);
        document.documentElement.style.background = "transparent";
        document.body.classList.add("mica-active");
      }

      // Register listeners before discovery starts so initial peer events cannot be missed.
      const listenerResults = await Promise.allSettled([
        onLanLog((level, text) => {
          const mapped =
            level === "success"
              ? "success"
              : level === "error"
                ? "error"
                : level === "warn"
                  ? "warn"
                  : "info";
          app.addLog(mapped as "info" | "warn" | "error" | "success", text);
        }),
        onLanPeerDiscovered((peer) => {
          app.upsertDevice(peer);
          const savedOutFolder = startupPeerFolders[peer.id];
          if (
            savedOutFolder &&
            app.devices.find((device) => device.id === peer.id)?.outFolder !== savedOutFolder
          ) {
            app.updateDeviceSettings(peer.id, { outFolder: savedOutFolder });
          }
          app.addLog("success", `Device discovered: ${peer.alias} (${peer.ip})`);
        }),
        onLanPeerLost((peerId) => {
          app.markDeviceOffline(peerId);
          const device = app.devices.find((d) => d.id === peerId);
          app.addLog("warn", `Device offline: ${device?.alias ?? peerId}`);
        }),
        onTailscaleStatus((status) => {
          app.tailscaleStatus = status;
        }),
        onLanTextReceived((peerId, text) => {
          revealIncomingPeer(peerId);
          app.addMessage({ peerId, direction: "received", text });
          app.addLog("info", `Received text from ${peerId.slice(0, 8)} (${text.length} chars)`);
          if (app.notificationsEnabled) playReceiveSound();
          if (app.notificationsEnabled && !document.hasFocus()) {
            sendNativeNotification(
              app.devices.find((device) => device.id === peerId)?.alias ?? "LanDrop",
              text || "New message received",
              { kind: "peer", peerId },
            ).catch((error) => app.addLog("warn", `Notification unavailable: ${error}`));
          }
          if (app.popOnReceive) windowShow();
        }),
        onLanFilesReceived((peerId, files, details) => {
          revealIncomingPeer(peerId);
          if (details.length > 0) {
            // Plain Map on purpose: a local grouping table, never reactive state.
            // eslint-disable-next-line svelte/prefer-svelte-reactivity
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
              const folderPath = getReceivedFolderPath(
                folderDetails[0].path,
                folderDetails[0].name,
              );
              const children = limitHistoryItems(folderDetails, MAX_HISTORY_ITEMS).map(
                (detail) => ({
                  name: detail.name,
                  path: detail.path,
                  size: fileSizeStr(detail.size),
                  type: fileIsImage(detail.name)
                    ? ("image" as const)
                    : fileIsVideo(detail.name)
                      ? ("video" as const)
                      : ("file" as const),
                }),
              );
              attachments.push({
                name: folder,
                path: folderPath,
                size: fileSizeStr(totalSize),
                type: "folder" as const,
                fileCount: folderDetails.length,
                children,
              });
            }
            for (const f of looseFiles) {
              attachments.push({
                name: f.name,
                path: f.path,
                size: fileSizeStr(f.size),
                type: fileIsImage(f.name)
                  ? ("image" as const)
                  : fileIsVideo(f.name)
                    ? ("video" as const)
                    : ("file" as const),
              });
            }
            app.addMessage({ peerId, direction: "received", text: "", attachments });
          }
          if (app.notificationsEnabled) playReceiveSound();
          if (app.notificationsEnabled && !document.hasFocus()) {
            const peerAlias =
              app.devices.find((device) => device.id === peerId)?.alias ?? "LanDrop";
            const body =
              files.length === 1 ? `Received ${files[0]}` : `Received ${files.length} items`;
            sendNativeNotification(peerAlias, body, { kind: "peer", peerId }).catch((error) =>
              app.addLog("warn", `Notification unavailable: ${error}`),
            );
          }
          if (app.popOnReceive) windowShow();
        }),
        onTransferProgress((progress) => {
          if (progress.direction === "receive") {
            if (progress.phase === "start") activeReceives += 1;
            if (progress.phase === "done" || progress.phase === "error") {
              activeReceives = Math.max(0, activeReceives - 1);
            }
            app.receivingTransferActive = activeReceives > 0;
          }
          const completedBytes = progress.sent_bytes ?? progress.received_bytes ?? 0;
          const totalBytes = progress.total_bytes ?? 0;
          if (progress.phase === "start") {
            lastProgressSample =
              completedBytes > 0
                ? { bytes: completedBytes, at: Date.now(), direction: progress.direction }
                : null;
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
          } else if (progress.phase === "done" || progress.phase === "error") {
            lastProgressSample = null;
            transferRateBps = null;
            transferEtaSeconds = null;
          }
          const terminal = progress.phase === "done" || progress.phase === "error";
          if (!terminal) {
            transferProgress = progress;
          } else if (!transferProgress || transferProgress.direction === progress.direction) {
            transferProgress = null;
          }
        }),
      ]);

      const registeredListeners = listenerResults.flatMap((result) =>
        result.status === "fulfilled" ? [result.value] : [],
      );
      if (disposed) {
        registeredListeners.forEach((unlisten) => unlisten());
        return;
      }
      unlisteners.push(...registeredListeners);

      const failedListener = listenerResults.find((result) => result.status === "rejected");
      if (failedListener?.status === "rejected") throw failedListener.reason;

      // Start mDNS discovery only after every frontend listener is ready.
      await startLanService();
      if (disposed) return;

      await repairStoredMessagePaths().catch((error) => {
        app.addLog("warn", `Could not repair stored message paths: ${error}`);
      });
      if (disposed) return;

      await setupHotkeys();
    })().catch((error) => {
      if (disposed) return;
      cleanupListeners();
      app.addLog("error", `Application startup failed: ${error}`);
      app.discoveryError = "LanDrop could not start. Check the debug log in Settings.";
      showSnackbar("LanDrop could not start. Check the debug log and try again.");
    });

    return () => {
      disposed = true;
      cleanupListeners();
    };
  });

  onMount(() => {
    if (isMobile()) return;
    let disposed = false;
    // Session-only deduplication, never rendered.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const announced = new Set<string>();
    async function checkForUpdatesQuietly() {
      if (disposed) return;
      await updater.check(async (version) => {
        if (disposed || !app.notificationsEnabled || announced.has(version)) return;
        announced.add(version);
        try {
          await sendNativeNotification(
            `LanDrop ${version} is available`,
            "View the update and choose when to install it.",
            { kind: "updates" },
          );
        } catch (error) {
          announced.delete(version);
          app.addLog("warn", `Update notification unavailable: ${error}`);
        }
      });
    }
    const startup = setTimeout(() => {
      void checkForUpdatesQuietly();
    }, 30_000);
    const periodic = setInterval(
      () => {
        void checkForUpdatesQuietly();
      },
      6 * 60 * 60 * 1000,
    );
    return () => {
      disposed = true;
      clearTimeout(startup);
      clearInterval(periodic);
      updater.dispose();
    };
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
    if (updater.phase === "downloading" || updater.phase === "installing") {
      showSnackbar("Wait for the app update to finish before sending.");
      return;
    }
    if (!app.hasFiles || app.transferActive) return;
    const device = app.activeDevice;
    if (!device) {
      showSnackbar("No device selected");
      return;
    }
    if (!device.online) {
      showSnackbar("Device is offline");
      return;
    }

    const filesCopy = [...app.files];
    const pathsCopy = [...app.filePaths];
    app.transferActive = true;
    try {
      let preparedFiles: PreparedSendPath[] = pathsCopy.map((path) => ({
        originalPath: path,
        sendPath: path,
        historyPath: path,
      }));
      const sent = await lanSendFiles(device.id, pathsCopy, device.ip, (prepared) => {
        preparedFiles = prepared;
      });
      if (sent) {
        const historyPathFor = new Map(
          preparedFiles.map((file) => [file.originalPath, file.historyPath]),
        );
        const attachments: MessageAttachment[] = limitHistoryItems(
          filesCopy,
          MAX_HISTORY_ITEMS,
        ).map((f) => ({
          name: f.info?.name ?? fileNameFromPath(f.path, "file"),
          path: historyPathFor.get(f.path) ?? f.path,
          size: f.info?.size_bytes ? fileSizeStr(f.info.size_bytes) : "",
          type: fileIsImage(f.info?.name ?? f.path)
            ? ("image" as const)
            : fileIsVideo(f.info?.name ?? f.path)
              ? ("video" as const)
              : ("file" as const),
        }));
        app.addMessage({ peerId: device.id, direction: "sent", text: "", attachments });
        app.removeSentFiles(filesCopy);
      } else {
        showSnackbar(`Send failed — ${device.alias} did not accept the transfer`);
        app.addLog("warn", `File transfer to ${device.alias} was not accepted`);
      }
    } catch (e) {
      showSnackbar("Send failed — " + e);
      app.markDeviceOffline(device.id);
      app.addLog("warn", `Send failed for ${device.alias} (${device.ip || "unknown ip"}): ${e}`);
    } finally {
      app.transferActive = false;
    }
  }

  async function handleSendText() {
    if (updater.phase === "downloading" || updater.phase === "installing") {
      showSnackbar("Wait for the app update to finish before sending.");
      return;
    }
    if (!app.sendTextContent.trim() || app.transferActive) return;
    const device = app.activeDevice;
    if (!device) {
      showSnackbar("No device selected");
      return;
    }
    if (!device.online) {
      showSnackbar("Device is offline");
      return;
    }

    const textDraft = app.sendTextContent;
    const textToSend = textDraft.trim();
    app.sendTextContent = "";

    function restoreText() {
      app.sendTextContent = app.sendTextContent.trim()
        ? `${textDraft}\n${app.sendTextContent}`
        : textDraft;
    }

    app.transferActive = true;
    try {
      const sent = await lanSendText(device.id, textToSend, device.ip);
      if (sent) {
        app.addMessage({ peerId: device.id, direction: "sent", text: textToSend });
      } else {
        restoreText();
        showSnackbar(`Send failed — ${device.alias} did not accept the message`);
        app.addLog("warn", `Message to ${device.alias} was not accepted`);
      }
    } catch (e: unknown) {
      restoreText();
      app.markDeviceOffline(device.id);
      const detail = e instanceof Error ? e.message : String(e ?? "device unreachable");
      showSnackbar(`Send failed — ${detail}`);
    } finally {
      app.transferActive = false;
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

<div
  class="app-shell"
  class:mica-on={theme.mica}
  style:--mica-opacity={theme.mica ? 0.05 + (theme.micaOpacity * 0.75) / 100 : 1}
>
  <!-- Custom title bar -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="titlebar" onmousedown={handleTitlebarMouseDown}>
    <div class="titlebar-content">
      {#if app.activeView === "transfer"}
        <div class="peer-strip">
          <PeerBar onedit={openDeviceSettings} onsnackbar={showSnackbar} />
        </div>
      {:else}
        <div class="flex items-center gap-2 flex-1 min-w-0">
          <Icon name="settings" size={18} />
          <span class="text-sm font-medium">Settings</span>
        </div>
      {/if}

      <IconButton
        title={app.activeView === "settings" ? "Back" : "Settings"}
        onclick={() => (app.activeView = app.activeView === "settings" ? "transfer" : "settings")}
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
      <TransferPage
        onsnackbar={showSnackbar}
        onsend={handleSendFiles}
        onsendtext={handleSendText}
      />
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
            <span class="progress-meta"
              >{[transferRateLabel, transferEtaLabel].filter(Boolean).join(" · ")}</span
            >
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
  onclose={() => {
    deviceDialogOpen = false;
    editingDevice = null;
  }}
/>

<Snackbar message={snackbarMsg} bind:visible={snackbarVisible} />

<Dialog
  bind:open={notificationDialogOpen}
  headline="Keep your current draft?"
  dismissLabel="Keep editing"
  confirmLabel="Discard draft and open"
  confirmDisabled={app.transferActive}
  onclose={() => {
    pendingNotificationPeer = null;
  }}
  onconfirm={() => {
    if (app.transferActive || !pendingNotificationPeer) return;
    app.sendTextContent = "";
    app.files = [];
    openNotificationConversation(pendingNotificationPeer);
    notificationDialogOpen = false;
    pendingNotificationPeer = null;
  }}
>
  <p>
    You have unsent text or attachments. Keep editing them, or discard them to open the
    notification's conversation.
  </p>
</Dialog>

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
    --icon-button-size: 40px;
    display: flex;
    align-items: stretch;
    background: var(--md-sys-color-surface);
    border-bottom: 1px solid
      color-mix(in srgb, var(--md-sys-color-outline-variant) 50%, transparent);
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
    /* Hint compositor for smoother scrolling on WebKitGTK (Linux) */
    contain: layout paint;
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
