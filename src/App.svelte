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
  import { getStatus, startLAN, stopLAN, lanSendText, lanSendFiles, onLanConnected, onLanDisconnected, onLanTextReceived, onLanFilesReceived, onTransferProgress, windowMinimize, windowToggleMaximize, windowClose, windowStartDrag } from "$lib/api/bridge";
  import type { TransferProgress } from "$lib/api/bridge";
  import { isImage as fileIsImage, fileSizeStr } from "$lib/utils/file-utils";

  import Icon from "$lib/ui/Icon.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import Snackbar from "$lib/ui/Snackbar.svelte";
  import PeerBar from "./features/peers/PeerBar.svelte";
  import PeerDialog from "./features/peers/PeerDialog.svelte";
  import TransferPage from "./features/transfer/TransferPage.svelte";
  import SettingsPage from "./features/settings/SettingsPage.svelte";

  const theme = getThemeState();
  const app = getAppState();

  let snackbarMsg = $state("");
  let snackbarVisible = $state(false);
  let appVersion = $state("1.0.0");
  let transferProgress = $state<TransferProgress | null>(null);

  let peerDialogOpen = $state(false);
  let editingPeer = $state<import("$lib/state/app-state.svelte").Peer | null>(null);

  function showSnackbar(msg: string) {
    snackbarMsg = msg;
    snackbarVisible = true;
  }

  function openAddPeer() {
    editingPeer = null;
    peerDialogOpen = true;
  }

  function openEditPeer(id: string) {
    editingPeer = app.peers.find(p => p.id === id) ?? null;
    peerDialogOpen = true;
  }

  $effect(() => {
    applyThemeToDOM(theme.tokens);
  });

  // LAN: start/stop based on active peer
  $effect(() => {
    const peer = app.activePeer;
    if (peer) {
      const outFolder = peer.outFolder ?? app.receiveOptions.outFolder ?? "";
      startLAN(peer.code, outFolder);
    } else {
      stopLAN();
      app.lanConnected = false;
      app.lanPeerIp = null;
    }
  });

  onMount(async () => {
    const status = await getStatus();
    app.localIp = status.local_ip ?? "unknown";
    if (status.app_version) appVersion = status.app_version;

    const unlisteners = await Promise.all([
      onLanConnected((peerIp) => {
        app.lanConnected = true;
        app.lanPeerIp = peerIp;
        app.addLog("success", `LAN connected to ${peerIp}`);
      }),
      onLanDisconnected(() => {
        app.lanConnected = false;
        app.lanPeerIp = null;
        app.addLog("warn", "LAN peer disconnected");
      }),
      onLanTextReceived((text) => {
        const peer = app.activePeer;
        if (peer) {
          app.addMessage({ peerId: peer.id, direction: "received", text });
          app.addActivity({ peerId: peer.id, direction: "received", type: "text", items: [], success: true });
        }
        showSnackbar("Message received!");
      }),
      onLanFilesReceived((files, details) => {
        const peer = app.activePeer;
        if (peer) {
          app.addActivity({ peerId: peer.id, direction: "received", type: "files", items: files, success: true, outFolder: app.effectiveOutFolder });
          if (details.length > 0) {
            const attachments: MessageAttachment[] = details.map(f => ({
              name: f.name, path: f.path, size: fileSizeStr(f.size),
              type: fileIsImage(f.name) ? "image" as const : "file" as const,
            }));
            app.addMessage({ peerId: peer.id, direction: "received", text: "", attachments });
          }
        }
        showSnackbar(files.length > 0 ? `Received ${files.join(", ")}` : "Files received!");
      }),
      onTransferProgress((progress) => {
        transferProgress = progress.phase === "done" ? null : progress;
      }),
    ]);

    if (!app.activePeerId && app.peers.length > 0) {
      app.setActivePeer(app.peers[0].id);
    }

    return () => { unlisteners.forEach(fn => fn()); };
  });

  async function handleSendFiles() {
    if (!app.hasFiles || app.transferActive) return;
    const peer = app.activePeer;
    if (peer) app.touchPeer(peer.id);
    if (!app.lanConnected) { showSnackbar("Not connected — waiting for LAN"); return; }

    const filesCopy = [...app.files];
    app.transferActive = true;
    try {
      const sent = await lanSendFiles(app.filePaths);
      if (sent && peer) {
        const names = filesCopy.map(f => f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file");
        app.addActivity({ peerId: peer.id, direction: "sent", type: "files", items: names, success: true });
        const attachments: MessageAttachment[] = filesCopy.map(f => ({
          name: f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file",
          path: f.path, size: f.info?.size ?? "",
          type: fileIsImage(f.info?.name ?? f.path) ? "image" as const : "file" as const,
        }));
        app.addMessage({ peerId: peer.id, direction: "sent", text: "", attachments });
        app.clearFiles();
      }
    } catch (e) {
      showSnackbar("Send failed — connection lost");
    } finally {
      app.transferActive = false;
    }
  }

  async function handleSendText() {
    if (!app.sendTextContent.trim() || app.transferActive) return;
    const peer = app.activePeer;
    if (peer) app.touchPeer(peer.id);
    const textToSend = app.sendTextContent.trim();
    if (peer) app.addMessage({ peerId: peer.id, direction: "sent", text: textToSend });
    app.sendTextContent = "";
    if (!app.lanConnected) { showSnackbar("Not connected — waiting for LAN"); return; }
    const sent = await lanSendText(textToSend);
    if (sent && peer) {
      app.addActivity({ peerId: peer.id, direction: "sent", type: "text", items: [], success: true });
    }
  }

  // Computed progress percentage
  const progressPct = $derived.by(() => {
    if (!transferProgress?.total_bytes || transferProgress.total_bytes === 0) return null;
    const done = transferProgress.sent_bytes ?? transferProgress.received_bytes ?? 0;
    return Math.round((done / transferProgress.total_bytes) * 100);
  });

  function handleTitlebarDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    // Only skip actual interactive elements, not empty background areas
    if (target.closest("button, input, a, [role='button'], .peer-chip, .titlebar-controls")) return;
    e.preventDefault();
    windowStartDrag();
  }

  function handleTitlebarDblClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest("button, input, a, [role='button'], .peer-chip, .titlebar-controls")) return;
    windowToggleMaximize();
  }
</script>

<div class="app-shell">
  <!-- Custom title bar -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="titlebar" onmousedown={handleTitlebarDrag} ondblclick={handleTitlebarDblClick}>
    <div class="titlebar-content">
      <div class="top-bar-brand">
        <div class="brand-icon">
          <Icon name="swap_horiz" size={16} />
        </div>
        <span class="brand-name">LanDrop</span>
      </div>

      {#if app.activeView === "transfer"}
        <div class="peer-strip">
          <PeerBar onadd={openAddPeer} onedit={openEditPeer} />
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
      <button class="titlebar-btn" onclick={windowMinimize} title="Minimize">
        <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
      </button>
      <button class="titlebar-btn" onclick={windowToggleMaximize} title="Maximize">
        <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1"/></svg>
      </button>
      <button class="titlebar-btn titlebar-btn-close" onclick={windowClose} title="Close">
        <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1.2"/></svg>
      </button>
    </div>
  </div>

  <!-- Content area -->
  <main class="content-area">
    {#if app.activeView === "transfer"}
      <TransferPage onsnackbar={showSnackbar} onaddpeer={openAddPeer} onsend={handleSendFiles} onsendtext={handleSendText} />
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

  <!-- Status Bar -->
  <footer class="status-bar">
    <div class="status-indicator" class:status-connected={app.lanConnected} class:status-searching={!app.lanConnected && !!app.activePeer}>
      <span class="status-dot"></span>
    </div>
    <span class="status-text">
      {app.lanConnected ? "Connected" : app.activePeer ? "Searching..." : "No peer selected"}
    </span>
    {#if app.lanConnected}
      <span class="status-badge">
        <Icon name="bolt" size={10} />
        LAN
      </span>
    {/if}
    <span class="flex-1"></span>
    <span class="status-ip">{app.localIp}</span>
  </footer>
</div>

<PeerDialog
  bind:open={peerDialogOpen}
  editPeer={editingPeer}
  onclose={() => { peerDialogOpen = false; editingPeer = null; }}
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
    padding: 8px 4px 8px 12px;
    flex: 1;
    min-width: 0;
    min-height: 48px;
  }
  .titlebar-controls {
    display: flex;
    align-items: stretch;
    flex-shrink: 0;
  }
  .titlebar-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 46px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--md-sys-color-on-surface-variant);
    transition: background 0.15s ease, color 0.15s ease;
  }
  .titlebar-btn:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .titlebar-btn-close:hover {
    background: #e81123;
    color: #fff;
  }
  .top-bar-brand {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    margin-right: 4px;
  }
  .brand-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }
  .brand-name {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.2px;
    color: var(--md-sys-color-on-surface);
    display: none; /* Hide on narrow widths */
  }
  @media (min-width: 480px) {
    .brand-name { display: inline; }
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
    transition: width 200ms ease-out;
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

  /* ── Status Bar ── */
  .status-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 16px;
    height: 32px;
    background: var(--md-sys-color-surface-container);
    border-top: 1px solid color-mix(in srgb, var(--md-sys-color-outline-variant) 50%, transparent);
    font-size: 11px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .status-indicator {
    position: relative;
    width: 8px;
    height: 8px;
  }
  .status-dot {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    background: var(--md-sys-color-outline);
    transition: background 300ms ease;
  }
  .status-connected .status-dot {
    background: var(--md-sys-color-primary);
    box-shadow: 0 0 6px color-mix(in srgb, var(--md-sys-color-primary) 60%, transparent);
    animation: pulse-glow 2s ease-in-out infinite;
  }
  .status-searching .status-dot {
    background: var(--md-sys-color-tertiary);
    animation: pulse-search 1.5s ease-in-out infinite;
  }
  @keyframes pulse-glow {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
  @keyframes pulse-search {
    0%, 100% { opacity: 0.3; transform: scale(0.8); }
    50% { opacity: 1; transform: scale(1); }
  }
  .status-text {
    font-weight: 500;
  }
  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--md-sys-color-primary) 15%, transparent);
    color: var(--md-sys-color-primary);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
  }
  .status-ip {
    font-family: var(--font-mono);
    font-size: 10px;
    opacity: 0.6;
  }
</style>
