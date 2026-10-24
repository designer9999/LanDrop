<!--
  Root Application Component — LanDrop
  M3 Expressive dark theme — elevated design
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { getThemeState } from "$lib/theme/theme-store.svelte";
  import { applyThemeToDOM } from "$lib/theme/apply-theme";
  import { getAppState } from "$lib/state/app-state.svelte";
  import type { MessageAttachment } from "$lib/state/model";
  import { ChatController } from "$lib/chat/chat-controller.svelte";
  import {
    getStatus,
    startLanService,
    onLanLog,
    onLanPeerDiscovered,
    onLanPeerLost,
    onTailscaleStatus,
    onLanTextReceived,
    onLanFilesReceived,
    onLanChatEvent,
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
  import { loadPersistedAppState, savePersistedAppState } from "$lib/persistence/app-store";
  import { fileSizeStr, joinReceivePath } from "$lib/utils/file-utils";
  import {
    sendNativeNotification,
    onNotificationActivation,
    type NotificationTarget,
  } from "$lib/utils/native-notifications";
  import { getUpdaterState } from "$lib/state/updater-state.svelte";
  import { openNotificationPeer } from "$lib/utils/notification-routing";

  import Icon from "$lib/ui/Icon.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import Snackbar from "$lib/ui/Snackbar.svelte";
  import ContextMenu from "$lib/ui/ContextMenu.svelte";
  import PeerBar from "./features/peers/PeerBar.svelte";
  import DeviceSettingsDialog from "./features/peers/DeviceSettingsDialog.svelte";
  import ConversationList from "./features/conversations/ConversationList.svelte";
  import Welcome from "./features/conversations/Welcome.svelte";
  import ChatView from "./features/chat/ChatView.svelte";
  import SettingsPage from "./features/settings/SettingsPage.svelte";

  const theme = getThemeState();
  const app = getAppState();
  const updater = getUpdaterState();

  function openNotificationConversation(peerId: string) {
    if (openNotificationPeer(app, peerId) === "missing") {
      showSnackbar("This conversation is no longer in your saved history.");
    }
    drawerOpen = false;
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

  let deviceDialogOpen = $state(false);
  let editingDevice = $state<import("$lib/state/app-state.svelte").DiscoveredDevice | null>(null);
  let persistedStateReady = $state(false);
  // Read-only display snapshot of Rust receive_folders.json, taken once at
  // startup; Rust remains the single owner (UI writes go through
  // set_peer_out_folder / set_default_out_folder at the moment of action).
  let startupPeerFolders: Record<string, string> = {};

  // Adaptive layout: a conversation list pane beside the chat on wide windows,
  // a drawer on narrow ones.
  let windowWidth = $state(window.innerWidth);
  const expanded = $derived(windowWidth >= 760);
  let drawerOpen = $state(false);

  const controller = new ChatController({
    app,
    notify: showSnackbar,
    updateInProgress: () => updater.phase === "downloading" || updater.phase === "installing",
  });

  function showSnackbar(msg: string) {
    snackbarMsg = msg;
    snackbarVisible = true;
  }

  function openDeviceSettings(id: string) {
    editingDevice = app.devices.find((d) => d.id === id) ?? null;
    deviceDialogOpen = true;
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
      savePersistedAppState(snapshot).catch((error) => {
        app.addLog("error", `Could not save history: ${error}`);
      });
    }, 300);
  });
  // Flush a pending save when the window is hidden or closed.
  function flushPersistedState() {
    if (!persistedStateReady) return;
    clearTimeout(persistedStateSaveTimer);
    void savePersistedAppState(app.exportPersistedState()).catch(() => {});
  }
  $effect(() => () => clearTimeout(persistedStateSaveTimer));

  onMount(() => {
    let unlisteners: Array<() => void> = [];
    let disposed = false;

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
        onLanTextReceived((event) => controller.handleText(event)),
        onLanFilesReceived((event) => controller.handleFiles(event)),
        onLanChatEvent((event) => controller.handleChatEvent(event)),
        onTransferProgress((progress) => controller.handleProgress(progress)),
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
    const target = app.activeDeviceId;
    for (const path of paths) {
      app.addFileTo(target, path, await getFileInfo(path));
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

  const legacy = $derived(controller.legacyTransfer);
  const legacyPct = $derived(
    legacy && legacy.total > 0 ? Math.round((legacy.bytes / legacy.total) * 100) : null,
  );

  // Reading the open conversation clears its unread badge.
  $effect(() => {
    const id = app.activeDeviceId;
    void app.messages.length;
    if (id && app.activeView === "transfer" && document.hasFocus()) app.markRead(id);
  });

  function handleTitlebarMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, input, a, [role='button'], .peer-chip, .no-drag")) return;
    if (e.detail === 2) {
      windowToggleMaximize();
    } else {
      windowStartDrag();
    }
  }
</script>

<svelte:window
  bind:innerWidth={windowWidth}
  onfocus={() =>
    app.activeDeviceId && app.activeView === "transfer" && app.markRead(app.activeDeviceId)}
  onblur={flushPersistedState}
  onbeforeunload={flushPersistedState}
/>

<div
  class="app-shell"
  class:mica-on={theme.mica}
  class:expanded
  style:--mica-opacity={theme.mica ? 0.05 + (theme.micaOpacity * 0.75) / 100 : 1}
>
  <!-- Custom title bar -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="titlebar" onmousedown={handleTitlebarMouseDown}>
    <div class="titlebar-content">
      {#if app.activeView === "settings"}
        <IconButton title="Back" onclick={() => (app.activeView = "transfer")}>
          <Icon name="arrow_back" size={20} />
        </IconButton>
        <span class="titlebar-title">Settings</span>
      {:else if expanded}
        <img class="titlebar-logo" src="/app-icon.png" alt="" width="22" height="22" />
        <span class="titlebar-title">LanDrop</span>
      {:else}
        <span class="drawer-button">
          <IconButton title="Conversations" onclick={() => (drawerOpen = true)}>
            <Icon name="menu" size={20} />
          </IconButton>
          {#if app.totalUnread > 0}<span class="drawer-badge">{app.totalUnread}</span>{/if}
        </span>
        <div class="peer-strip">
          <PeerBar onedit={openDeviceSettings} onsnackbar={showSnackbar} />
        </div>
      {/if}
      <span class="flex-1"></span>
      {#if app.activeView !== "settings"}
        <IconButton title="Settings" onclick={() => (app.activeView = "settings")}>
          <Icon name="settings" size={20} />
        </IconButton>
      {/if}
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

  <main class="content-area">
    {#if app.activeView === "settings"}
      <div class="settings-scroll">
        <div class="settings-column">
          <SettingsPage {appVersion} onsnackbar={showSnackbar} />
        </div>
      </div>
    {:else}
      {#if expanded}
        <aside class="list-pane">
          <ConversationList onsnackbar={showSnackbar} />
        </aside>
      {/if}
      <div class="chat-pane">
        {#if app.activeDevice}
          {#key app.activeDevice.id}
            <ChatView
              device={app.activeDevice}
              {controller}
              compact={!expanded}
              onsnackbar={showSnackbar}
              ondevicesettings={openDeviceSettings}
            />
          {/key}
        {:else if app.conversations.length > 0}
          <div class="pick">
            <Icon name={app.unassignedFileCount ? "outbox" : "forum"} size={36} />
            <p>
              {app.unassignedFileCount
                ? `${app.unassignedFileCount} file${app.unassignedFileCount === 1 ? " is" : "s are"} ready — choose who to send ${app.unassignedFileCount === 1 ? "it" : "them"} to.`
                : "Choose a device to start chatting."}
            </p>
            {#if !expanded}
              <button class="pick-button" onclick={() => (drawerOpen = true)}>Show devices</button>
            {/if}
          </div>
        {:else}
          <Welcome />
        {/if}
      </div>
    {/if}
  </main>

  {#if legacy}
    <div class="progress-bar" role="status">
      <div class="progress-bar-fill" style="width: {legacyPct ?? 0}%"></div>
      <div class="progress-bar-content">
        <Icon name={legacy.direction === "send" ? "upload" : "download"} size={14} />
        <span class="truncate flex-1">
          {legacy.direction === "send" ? "Sending" : "Receiving"}
          {legacy.currentFile?.split("/").pop() ?? ""}
          {#if legacy.total}· {fileSizeStr(legacy.bytes)} of {fileSizeStr(legacy.total)}{/if}
        </span>
        {#if legacyPct !== null}<span class="progress-pct">{legacyPct}%</span>{/if}
      </div>
    </div>
  {/if}
</div>

{#if drawerOpen && !expanded && app.activeView !== "settings"}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="scrim" onclick={() => (drawerOpen = false)}></div>
  <aside class="drawer" aria-label="Conversations">
    <ConversationList onselect={() => (drawerOpen = false)} onsnackbar={showSnackbar} />
  </aside>
{/if}

<DeviceSettingsDialog
  bind:open={deviceDialogOpen}
  device={editingDevice}
  onclose={() => {
    deviceDialogOpen = false;
    editingDevice = null;
  }}
/>

<ContextMenu />
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
  .titlebar-title {
    font-size: 15px;
    font-weight: 600;
    font-variation-settings: "wdth" 112;
    padding-left: 4px;
  }
  .titlebar-logo {
    margin-left: 8px;
    border-radius: 6px;
  }
  .drawer-button {
    position: relative;
    display: inline-flex;
  }
  .drawer-badge {
    position: absolute;
    top: 0;
    right: 0;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    font-size: 10px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    background: var(--md-sys-color-error);
    color: var(--md-sys-color-on-error);
    pointer-events: none;
  }

  /* ── Panes ── */
  .list-pane {
    width: clamp(260px, 30vw, 340px);
    flex-shrink: 0;
    min-height: 0;
    background: var(--md-sys-color-surface-container-low);
    border-right: 1px solid color-mix(in srgb, var(--md-sys-color-outline-variant) 50%, transparent);
  }
  .chat-pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .pick {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--md-sys-color-on-surface-variant);
    font-size: 14px;
  }
  .pick p {
    margin: 0;
  }
  .pick-button {
    margin-top: 8px;
    height: 40px;
    padding: 0 20px;
    border: none;
    border-radius: 20px;
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    font: inherit;
    font-weight: 500;
    cursor: pointer;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 40%, transparent);
    animation: fade-in 150ms ease-out;
  }
  .drawer {
    position: fixed;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 201;
    width: min(340px, 86vw);
    background: var(--md-sys-color-surface-container-low);
    border-radius: 0 28px 28px 0;
    box-shadow: var(--shadow-level3);
    animation: drawer-in var(--md-spring-default-spatial-dur) var(--md-spring-default-spatial);
    overflow: hidden;
  }
  @keyframes drawer-in {
    from {
      transform: translateX(-100%);
    }
  }
  @keyframes fade-in {
    from {
      opacity: 0;
    }
  }

  /* ── Content ── */
  .content-area {
    flex: 1;
    display: flex;
    flex-direction: row;
    min-height: 0;
  }
  .settings-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    /* Hint compositor for smoother scrolling on WebKitGTK (Linux) */
    contain: layout paint;
  }

  .settings-column {
    max-width: 760px;
    margin: 0 auto;
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
  .mica-on .list-pane {
    background: rgba(255, 255, 255, 0.04);
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
