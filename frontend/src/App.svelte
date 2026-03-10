<!--
  Root Application Component — Dashboard layout
  Contact bar + Transfer/Settings views + Status bar
  Theme applied on mount via M3 Expressive (SchemeExpressive)
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { getThemeState } from "$lib/theme/theme-store.svelte";
  import { applyThemeToDOM } from "$lib/theme/apply-theme";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { getStatus, stopTransfer, checkUpdate, downloadUpdate, launchUpdate, installCroc, sendFiles, sendText, startLAN, stopLAN, lanSendText, lanSendFiles, setFocused, setNotifications, getFileInfo } from "$lib/api/bridge";
  import type { MessageAttachment } from "$lib/state/app-state.svelte";

  const IMAGE_EXTS = new Set([".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".ico", ".svg"]);
  function fileIsImage(name: string): boolean {
    const ext = name.slice(name.lastIndexOf(".")).toLowerCase();
    return IMAGE_EXTS.has(ext);
  }
  function fileSizeStr(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }
  import Icon from "$lib/ui/Icon.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Snackbar from "$lib/ui/Snackbar.svelte";
  import Divider from "$lib/ui/Divider.svelte";
  import ContactBar from "./features/contacts/ContactBar.svelte";
  import ContactDialog from "./features/contacts/ContactDialog.svelte";
  import TransferPage from "./features/transfer/TransferPage.svelte";
  import SettingsPage from "./features/settings/SettingsPage.svelte";

  const theme = getThemeState();
  const app = getAppState();

  let snackbarMsg = $state("");
  let snackbarVisible = $state(false);

  let appVersion = $state("2.0.0");
  let updateAvailable = $state(false);
  let updateUrl = $state("");
  let updateVersion = $state("");
  let updateDownloading = $state(false);
  let updateProgress = $state(0);
  let updateFilePath = $state("");

  // Contact dialog state
  let contactDialogOpen = $state(false);
  let editingContact = $state<import("$lib/state/app-state.svelte").Contact | null>(null);

  function showSnackbar(msg: string) {
    snackbarMsg = msg;
    snackbarVisible = true;
  }

  function openAddContact() {
    editingContact = null;
    contactDialogOpen = true;
  }

  function openEditContact(id: string) {
    editingContact = app.contacts.find(c => c.id === id) ?? null;
    contactDialogOpen = true;
  }

  $effect(() => {
    applyThemeToDOM(theme.tokens);
  });

  // LAN direct: start/stop based on active contact
  $effect(() => {
    const contact = app.activeContact;
    if (contact && app.crocOk) {
      const outFolder = contact.options?.outFolder ?? app.receiveOptions.outFolder ?? "";
      startLAN(contact.code, outFolder);
    } else {
      stopLAN();
      app.lanConnected = false;
      app.lanPeerIp = null;
    }
  });


  onMount(async () => {
    const status = await getStatus();
    app.crocOk = status.ok;
    app.crocVersion = status.version?.replace("croc version ", "") ?? "not found";
    app.localIp = status.local_ip ?? "unknown";
    if (status.app_version) appVersion = status.app_version;

    // Track focus state for notifications
    window.addEventListener("focus", () => setFocused(true));
    window.addEventListener("blur", () => setFocused(false));
    setNotifications(app.notificationsEnabled);

    window.onLog = (level: string, msg: string) => {
      const clean = msg.replace(/\x1b\[[0-9;]*m/g, "").replace(/\r/g, "");
      app.addLog(level as any, clean);
    };

    window.onEvent = (event: string, data: any) => {
      switch (event) {
        case "transfer_start":
          app.transferActive = true;
          app.transferMode = data.mode;
          app.transferCode = null;
          break;

        case "code_ready":
          app.transferCode = data.code;
          break;

        case "transfer_done": {
          const success = data.success;
          const fileNames: string[] = data.files ?? [];
          const direction = data.mode === "send" ? "sent" : "received";
          const isText = fileNames.length === 1 && fileNames[0] === "(text)";

          if (success) {
            const summary = isText
              ? (direction === "sent" ? "Text sent!" : "Text received!")
              : fileNames.length > 0
                ? `${direction === "sent" ? "Sent" : "Received"} ${fileNames.join(", ")}`
                : "Transfer complete!";
            app.addLog("success", summary);
            // Only show snackbar for received items — sender doesn't need popups
            if (direction === "received") {
              showSnackbar(summary);
            }

            const contact = app.activeContact;
            if (contact) {
              app.addActivity({
                contactId: contact.id,
                direction,
                type: isText ? "text" : "files",
                items: isText ? [] : fileNames,
                success: true,
                outFolder: direction === "received" ? app.effectiveReceiveOptions.outFolder : undefined,
              });
            }

            // Auto-clear sent files after successful send
            if (direction === "sent" && !isText) {
              app.clearFiles();
            }
          } else if (!data.stopped) {
            showSnackbar("Transfer failed");
            const contact = app.activeContact;
            if (contact) {
              app.addActivity({
                contactId: contact.id,
                direction,
                type: isText ? "text" : "files",
                items: fileNames,
                success: false,
              });
            }
          }
          app.resolveTransfer(success);
          break;
        }

        case "update_progress":
          updateProgress = data.percent;
          break;

        case "update_ready":
          updateDownloading = false;
          updateFilePath = data.path;
          showSnackbar("Update downloaded! Click to install.");
          break;

        case "update_failed":
          updateDownloading = false;
          showSnackbar("Update download failed");
          break;

        case "lan_connected":
          app.lanConnected = true;
          app.lanPeerIp = data.peer_ip;
          break;

        case "lan_disconnected":
          app.lanConnected = false;
          app.lanPeerIp = null;
          break;

        case "lan_text_received": {
          const lanContact = app.activeContact;
          if (lanContact) {
            app.addMessage({
              contactId: lanContact.id,
              direction: "received",
              text: data.text,
            });
            app.addActivity({
              contactId: lanContact.id,
              direction: "received",
              type: "text",
              items: [],
              success: true,
            });
          }
          showSnackbar("Message received!");
          break;
        }

        case "lan_files_received": {
          const lanFileContact = app.activeContact;
          const lanFiles: string[] = data.files ?? [];
          const fileDetails: Array<{name: string; path: string; size: number}> = data.file_details ?? [];
          if (lanFileContact) {
            app.addActivity({
              contactId: lanFileContact.id,
              direction: "received",
              type: "files",
              items: lanFiles,
              success: true,
              outFolder: app.effectiveReceiveOptions.outFolder,
            });
            // Add as chat message with attachments
            if (fileDetails.length > 0) {
              const attachments: MessageAttachment[] = fileDetails.map(f => ({
                name: f.name,
                path: f.path,
                size: fileSizeStr(f.size),
                type: fileIsImage(f.name) ? "image" as const : "file" as const,
              }));
              app.addMessage({
                contactId: lanFileContact.id,
                direction: "received",
                text: "",
                attachments,
              });
            }
          }
          const lanSummary = lanFiles.length > 0
            ? `Received ${lanFiles.join(", ")}`
            : "Files received!";
          showSnackbar(lanSummary);
          break;
        }

        case "files_dropped": {
          const paths: string[] = data.paths ?? [];
          for (const p of paths) {
            getFileInfo(p).then(info => app.addFile(p, info));
          }
          break;
        }

        case "install_croc_start":
          app.crocInstalling = true;
          break;

        case "install_croc_done":
          app.crocInstalling = false;
          if (data.success) {
            if (data.needs_restart) {
              showSnackbar("croc installed! Restart the app.");
            } else {
              app.crocOk = true;
              showSnackbar("croc installed successfully!");
              getStatus().then(s => {
                app.crocVersion = s.version?.replace("croc version ", "") ?? "installed";
              });
            }
          } else if (data.no_winget) {
            showSnackbar("winget not found — install croc manually");
          } else {
            showSnackbar("croc installation failed");
          }
          break;
      }
    };

    // Check for updates silently
    try {
      const info = await checkUpdate();
      if (info.update_available && info.download_url) {
        updateAvailable = true;
        updateUrl = info.download_url;
        updateVersion = info.latest_version ?? "";
      }
    } catch (_e) { /* silent fail */ }

    // Auto-select first contact if none selected
    if (!app.activeContactId && app.contacts.length > 0) {
      app.setActiveContact(app.contacts[0].id);
    }
  });

  // Split send — files and text are separate actions
  const fabHasFiles = $derived(app.hasFiles);
  const fabHasText = $derived(!!app.sendTextContent.trim());
  const fabShowSplit = $derived(fabHasFiles && fabHasText);

  async function handleSendFiles() {
    if (!app.hasFiles || app.transferActive) return;
    const contact = app.activeContact;
    if (contact) app.touchContact(contact.id);

    // LAN direct — instant transfer (no croc fallback when connected)
    if (app.lanConnected) {
      const filesCopy = [...app.files];
      const sent = await lanSendFiles(app.filePaths);
      if (sent) {
        if (contact) {
          const names = filesCopy.map(f => f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file");
          app.addActivity({
            contactId: contact.id,
            direction: "sent",
            type: "files",
            items: names,
            success: true,
          });
          // Add as chat message with attachments
          const attachments: MessageAttachment[] = filesCopy.map(f => ({
            name: f.info?.name ?? f.path.split(/[\\/]/).pop() ?? "file",
            path: f.path,
            size: f.info?.size ?? "",
            type: fileIsImage(f.info?.name ?? f.path) ? "image" as const : "file" as const,
          }));
          app.addMessage({
            contactId: contact.id,
            direction: "sent",
            text: "",
            attachments,
          });
        }
        app.clearFiles();
      }
      return;
    }

    // No LAN connection — use croc
    const opts = app.effectiveSendOptions;
    await sendFiles(app.filePaths, opts);
  }

  async function handleSendText() {
    if (!app.sendTextContent.trim() || app.transferActive) return;
    const contact = app.activeContact;
    if (contact) app.touchContact(contact.id);
    const textToSend = app.sendTextContent.trim();
    if (contact) app.addMessage({ contactId: contact.id, direction: "sent", text: textToSend });
    app.sendTextContent = "";

    // LAN direct — instant delivery (no croc fallback when connected)
    if (app.lanConnected) {
      const sent = await lanSendText(textToSend);
      if (sent && contact) {
        app.addActivity({
          contactId: contact.id,
          direction: "sent",
          type: "text",
          items: [],
          success: true,
        });
      }
      return;
    }

    // No LAN connection — use croc
    const opts = app.effectiveSendOptions;
    await sendText(textToSend, opts);
  }

  async function handleSendAll() {
    if (!app.canSend || app.transferActive) return;
    if (app.hasFiles) {
      await handleSendFiles();
    } else if (fabHasText) {
      await handleSendText();
    }
  }

  async function handleDownloadUpdate() {
    if (!updateUrl) return;
    updateDownloading = true;
    updateProgress = 0;
    await downloadUpdate(updateUrl);
  }

  async function handleInstallUpdate() {
    if (updateFilePath) await launchUpdate(updateFilePath);
  }
</script>

<div class="flex flex-col h-screen bg-surface text-on-surface overflow-hidden select-none">

  <!-- Top bar — title + contacts + settings on ONE row -->
  <div class="top-bar">
    <span class="text-primary shrink-0"><Icon name="swap_horiz" size={20} /></span>
    {#if app.activeView === "transfer"}
      <div class="contact-strip">
        <ContactBar onadd={openAddContact} onedit={openEditContact} />
      </div>
    {:else}
      <span class="text-sm font-medium text-on-surface whitespace-nowrap">Settings</span>
      <span class="flex-1"></span>
    {/if}
    <span class="text-[10px] px-1.5 py-0.5 rounded-full bg-surface-container-high text-on-surface-variant shrink-0">
      v{appVersion}
    </span>
    <IconButton
      title="Settings"
      onclick={() => app.activeView = app.activeView === "settings" ? "transfer" : "settings"}
    >
      <Icon name={app.activeView === "settings" ? "close" : "settings"} size={18} />
    </IconButton>
  </div>
  <Divider />

  <!-- Update banner -->
  {#if updateAvailable && !updateDownloading && !updateFilePath}
    <div class="flex items-center gap-2 px-4 py-1.5 bg-tertiary-container text-on-tertiary-container text-xs">
      <Icon name="system_update" size={16} />
      <span class="flex-1">Update {updateVersion} available</span>
      <button
        class="text-xs font-medium px-2.5 py-0.5 rounded-full bg-tertiary text-on-tertiary
               cursor-pointer border-none"
        onclick={handleDownloadUpdate}
      >Download</button>
    </div>
  {/if}

  {#if updateDownloading}
    <div class="flex items-center gap-2 px-4 py-1.5 bg-tertiary-container text-on-tertiary-container text-xs">
      <Icon name="downloading" size={16} />
      <span class="flex-1">Downloading... {updateProgress}%</span>
    </div>
    <div class="h-0.5 bg-surface-container-highest">
      <div class="h-full bg-tertiary" style="width: {updateProgress}%; transition: width 200ms ease;"></div>
    </div>
  {/if}

  {#if updateFilePath}
    <div class="flex items-center gap-2 px-4 py-1.5 bg-primary-container text-on-primary-container text-xs">
      <Icon name="check_circle" size={16} />
      <span class="flex-1">Update ready</span>
      <button
        class="text-xs font-medium px-2.5 py-0.5 rounded-full bg-primary text-on-primary
               cursor-pointer border-none"
        onclick={handleInstallUpdate}
      >Install & restart</button>
    </div>
  {/if}

  <!-- croc not found banner -->
  {#if !app.crocOk && !app.crocInstalling}
    <div class="flex items-center gap-2 px-4 py-1.5 bg-error-container text-on-error-container text-xs">
      <Icon name="error" size={16} />
      <span class="flex-1">croc not found</span>
      <button
        class="text-xs font-medium px-2.5 py-0.5 rounded-full bg-error text-on-error
               cursor-pointer border-none"
        onclick={async () => await installCroc()}
      >Install</button>
    </div>
  {/if}

  {#if app.crocInstalling}
    <div class="flex items-center gap-2 px-4 py-1.5 bg-tertiary-container text-on-tertiary-container text-xs">
      <Icon name="hourglass_empty" size={16} />
      <span class="flex-1">Installing croc...</span>
    </div>
  {/if}

  <!-- Content -->
  <div class="flex-1 overflow-y-auto p-4">
    {#if app.activeView === "transfer"}
      <TransferPage onsnackbar={showSnackbar} onaddcontact={openAddContact} />
    {:else}
      <SettingsPage {appVersion} onsnackbar={showSnackbar} />
    {/if}
  </div>

  <!-- FAB Send / Stop — always visible at bottom -->
  {#if app.activeView === "transfer"}
    <div class="px-4 py-2 border-t border-outline-variant bg-surface">
      {#if app.transferActive}
        <Button variant="error" full onclick={stopTransfer}>
          <Icon name="stop" size={18} />
          Stop Transfer
        </Button>
      {:else if fabShowSplit}
        <!-- M3 Split Button: two separate rounded segments with gap -->
        <div class="split-btn">
          <button class="split-btn-leading" onclick={handleSendFiles}>
            <div class="split-btn-state"></div>
            <span class="split-btn-content">
              <Icon name="upload_file" size={18} />
              <span>Send files</span>
            </span>
          </button>
          <button class="split-btn-trailing" onclick={handleSendText}>
            <div class="split-btn-state"></div>
            <span class="split-btn-content">
              <Icon name="chat" size={18} />
              <span>Send text</span>
            </span>
          </button>
        </div>
      {:else if app.canSend}
        <Button full onclick={handleSendAll}>
          <Icon name="send" size={18} />
          {fabHasText ? "Send text" : "Send"}
        </Button>
      {/if}
    </div>
  {/if}

  <!-- Status Bar -->
  <div class="flex items-center gap-2 px-4 py-2.5
              bg-surface-container border-t border-outline-variant
              text-xs text-on-surface-variant">
    <div
      class="w-2 h-2 rounded-full
             {app.transferActive ? 'bg-primary animate-pulse'
               : app.crocOk ? 'bg-tertiary'
               : 'bg-error'}"
    ></div>
    <span>
      {app.transferActive
        ? app.transferMode === "send" ? "Sending..." : "Receiving..."
        : app.crocOk ? "Ready" : "croc not found"}
    </span>
    {#if app.lanConnected}
      <span class="text-primary flex items-center gap-1">
        <Icon name="bolt" size={12} />
        LAN direct
      </span>
    {/if}
    <span class="flex-1"></span>
    <span>croc {app.crocVersion}</span>
  </div>
</div>

<!-- Contact Dialog -->
<ContactDialog
  bind:open={contactDialogOpen}
  editContact={editingContact}
  onclose={() => { contactDialogOpen = false; editingContact = null; }}
/>

<Snackbar message={snackbarMsg} bind:visible={snackbarVisible} />

<style>
  /* ── Top bar — single row with contacts inline ── */
  .top-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 2px 10px;
    min-height: 36px;
  }
  .contact-strip {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  /* M3 Split Button — two separate segments with gap */
  .split-btn {
    display: flex;
    align-items: center;
    width: 100%;
    height: 40px;
    gap: 2px;
  }

  /* Shared segment styles */
  .split-btn-leading,
  .split-btn-trailing {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    border: none;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    cursor: pointer;
    user-select: none;
    outline: none;
    overflow: hidden;
    box-shadow: var(--shadow-level0);
    transition: box-shadow var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .split-btn-leading:hover,
  .split-btn-trailing:hover {
    box-shadow: var(--shadow-level1);
  }

  /* Leading segment — pill left, flat right */
  .split-btn-leading {
    flex: 1;
    border-radius: 9999px 4px 4px 9999px;
    padding: 0 20px 0 24px;
  }

  /* Trailing segment — flat left, pill right */
  .split-btn-trailing {
    flex: 1;
    border-radius: 4px 9999px 9999px 4px;
    padding: 0 24px 0 20px;
  }

  /* State layer — M3 spring animation */
  .split-btn-state {
    position: absolute;
    inset: 0;
    background: var(--md-sys-color-on-primary);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .split-btn-leading:hover .split-btn-state,
  .split-btn-trailing:hover .split-btn-state {
    opacity: 0.08;
  }
  .split-btn-leading:focus-visible .split-btn-state,
  .split-btn-trailing:focus-visible .split-btn-state {
    opacity: 0.1;
  }
  .split-btn-leading:active .split-btn-state,
  .split-btn-trailing:active .split-btn-state {
    opacity: 0.1;
  }

  /* Content — label + icon */
  .split-btn-content {
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 500;
    letter-spacing: 0.1px;
    white-space: nowrap;
  }
</style>
