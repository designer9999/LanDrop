<!--
  Composer for the open conversation: reply/edit banner, attachment tray,
  auto-growing field and send. Paste accepts any mix of images, videos and
  copied files; Enter sends, Shift+Enter adds a line, ↑ edits your last
  message, Esc cancels a reply or edit.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import FileCard from "../transfer/FileCard.svelte";
  import MediaThumb from "./MediaThumb.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { getClipboardFiles, getFileInfo, isMobile, saveClipboardImage } from "$lib/api/bridge";
  import { attachmentType } from "$lib/chat/attachments";
  import { messagePreview } from "$lib/chat/message-reducers";
  import { fileExtensionLabel, fileNameFromPath, fileSizeStr } from "$lib/utils/file-utils";
  import { menu } from "$lib/ui/menu-state.svelte";
  import { getChatContext } from "./chat-context";

  interface Props {
    peerAlias: string;
    onpickfiles: () => void;
    onpickfolder: () => void;
    onaddpaths: (paths: string[]) => Promise<void>;
  }

  let { peerAlias, onpickfiles, onpickfolder, onaddpaths }: Props = $props();

  const app = getAppState();
  const chat = getChatContext();
  const mobile = isMobile();

  let field: HTMLTextAreaElement | undefined = $state();
  let pasting = $state(0);

  const online = $derived(app.activeDeviceOnline);
  const editing = $derived(app.editingMessage);
  const replying = $derived(app.replyingTo);
  const hasContent = $derived(!!app.sendTextContent.trim() || app.hasFiles);
  const canSend = $derived(
    online && pasting === 0 && (editing ? !!app.sendTextContent.trim() : hasContent),
  );
  const media = $derived(
    app.files.filter((f) => {
      const type = attachmentType(f.info?.name ?? f.path);
      return f.info?.type !== "folder" && (type === "image" || type === "video");
    }),
  );
  const others = $derived(app.files.filter((f) => !media.includes(f)));

  export function focus() {
    field?.focus();
  }

  $effect(() => {
    // Focus the field when a reply or edit starts, or the conversation changes.
    void app.activeDeviceId;
    void app.draft.replyTo;
    void app.draft.editingId;
    if (!mobile) queueMicrotask(() => field?.focus());
  });

  function send() {
    if (!canSend) return;
    void chat.controller.sendDraft();
  }

  function cancelMode() {
    if (editing) app.stopEditing();
    else if (replying) app.setReplyTo(null);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key === "Enter" && !event.shiftKey && !mobile) {
      event.preventDefault();
      send();
    } else if (event.key === "Escape" && (editing || replying)) {
      event.preventDefault();
      cancelMode();
    } else if (
      event.key === "ArrowUp" &&
      !app.sendTextContent &&
      !app.hasFiles &&
      app.activeDeviceId
    ) {
      const last = app.lastEditableMessage(app.activeDeviceId);
      if (last) {
        event.preventDefault();
        app.startEditing(last.id);
      }
    }
  }

  async function handlePaste(event: ClipboardEvent) {
    const data = event.clipboardData;
    if (!data || editing) return;

    // Image/video bytes (screenshots, "Copy image" in browsers, phone keyboards).
    const blobs = [...data.items]
      .filter((item) => item.kind === "file" && /^(image|video)\//.test(item.type))
      .map((item) => item.getAsFile())
      .filter((file): file is File => !!file);

    if (blobs.length > 0) {
      event.preventDefault();
      pasting += blobs.length;
      for (const blob of blobs) {
        try {
          const path = await saveClipboardImage(blob, blob.type);
          app.addFile(path, await getFileInfo(path));
        } catch (error) {
          chat.notify(`Couldn't paste: ${error instanceof Error ? error.message : error}`);
        } finally {
          pasting -= 1;
        }
      }
      return;
    }

    // Files copied in Explorer / Finder arrive as paths, not bytes.
    if (!mobile && ![...data.types].includes("text/plain")) {
      event.preventDefault();
    }
    try {
      const paths = await getClipboardFiles();
      if (paths.length > 0) {
        event.preventDefault();
        await onaddpaths(paths);
      }
    } catch {
      // Plain text: the browser's default paste already happened.
    }
  }

  function openAttachMenu(anchor: HTMLElement) {
    menu.openAt(anchor, [
      {
        label: mobile ? "Photos, videos & files" : "Files",
        icon: "attach_file",
        onselect: onpickfiles,
      },
      { label: "Folder", icon: "create_new_folder", onselect: onpickfolder },
    ]);
  }
</script>

<div class="composer" class:composer-offline={!online}>
  {#if editing || replying}
    <div class="banner">
      <Icon name={editing ? "edit" : "reply"} size={20} />
      <div class="banner-body">
        <span class="banner-title">
          {editing
            ? "Edit message"
            : `Reply to ${replying?.direction === "sent" ? "yourself" : peerAlias}`}
        </span>
        <span class="banner-text">{messagePreview(editing ?? replying ?? undefined)}</span>
      </div>
      <button
        class="banner-close"
        onclick={cancelMode}
        aria-label={editing ? "Cancel edit" : "Cancel reply"}
        title="Esc"
      >
        <Icon name="close" size={18} />
      </button>
    </div>
  {/if}

  {#if app.hasFiles && !editing}
    <div class="tray" aria-label="Attachments">
      {#each media as file (file.path)}
        {@const name = file.info?.name ?? fileNameFromPath(file.path, "file")}
        <div class="tray-media">
          <MediaThumb
            attachment={{ name, path: file.path, type: attachmentType(name) }}
            onopen={() => chat.openFilePreview(file.path)}
          />
          <button
            class="tray-remove"
            onclick={() => app.removeFile(file.path)}
            aria-label="Remove {name}"
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      {/each}
      {#each others as file (file.path)}
        {@const name = file.info?.name ?? fileNameFromPath(file.path, "file")}
        <FileCard
          compact
          icon={file.info?.type === "folder" ? "folder" : "draft"}
          {name}
          size={file.info?.size_bytes ? fileSizeStr(file.info.size_bytes) : undefined}
          badge={file.info?.type === "folder" ? "FOLDER" : fileExtensionLabel(name)}
          onclick={() => chat.openFilePreview(file.path)}
        >
          {#snippet action()}
            <button
              class="tray-remove tray-remove-card"
              onclick={(event) => {
                event.stopPropagation();
                app.removeFile(file.path);
              }}
              aria-label="Remove {name}"
            >
              <Icon name="close" size={14} />
            </button>
          {/snippet}
        </FileCard>
      {/each}
      {#if pasting > 0}
        <div class="tray-media tray-pending"><Icon name="hourglass_top" size={20} /></div>
      {/if}
    </div>
  {/if}

  <div class="bar">
    {#if !editing}
      <button
        class="attach"
        onclick={(event) => openAttachMenu(event.currentTarget)}
        disabled={!online}
        aria-label="Attach files"
        title="Attach files"
      >
        <Icon name="add" size={24} />
      </button>
    {/if}
    <div class="field-wrap">
      <textarea
        bind:this={field}
        bind:value={app.sendTextContent}
        class="field"
        rows="1"
        placeholder={online
          ? editing
            ? "Edit your message"
            : `Message ${peerAlias}`
          : `${peerAlias} is offline`}
        disabled={!online && !app.sendTextContent}
        onkeydown={handleKeydown}
        onpaste={handlePaste}
        aria-label="Message"
        spellcheck="true"></textarea>
    </div>
    <button
      class="send"
      class:send-ready={canSend}
      disabled={!canSend}
      onclick={send}
      aria-label={editing ? "Save edit" : "Send"}
      title={editing ? "Save edit (Enter)" : "Send (Enter)"}
    >
      <Icon name={editing ? "check" : "send"} size={22} />
    </button>
  </div>
</div>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 12px 12px;
    max-width: 920px;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 6px 8px 14px;
    border-radius: 16px;
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-primary);
    animation: rise var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  .banner-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border-left: 3px solid var(--md-sys-color-primary);
    padding-left: 10px;
  }
  .banner-title {
    font-size: 12px;
    font-weight: 600;
  }
  .banner-text {
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .banner-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border: none;
    border-radius: 18px;
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
  }
  .banner-close:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }

  .tray {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    padding: 4px 2px 6px;
    scrollbar-width: thin;
  }
  .tray-media {
    position: relative;
    flex-shrink: 0;
    width: 76px;
    height: 76px;
    border-radius: 16px;
    overflow: hidden;
    animation: rise var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .tray-pending {
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
  }
  .tray-remove {
    position: absolute;
    top: 4px;
    right: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 11px;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 65%, transparent);
    color: white;
    cursor: pointer;
    z-index: 1;
  }
  .tray-remove-card {
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface-variant);
  }

  .bar {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }
  .attach {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 48px;
    height: 48px;
    border: none;
    border-radius: 16px;
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    cursor: pointer;
    transition: border-radius var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .attach:hover:not(:disabled) {
    border-radius: 24px;
  }
  .attach:disabled {
    opacity: 0.38;
    cursor: default;
  }
  .field-wrap {
    flex: 1;
    min-width: 0;
    display: flex;
    border-radius: 24px;
    background: var(--md-sys-color-surface-container-high);
    transition: box-shadow 160ms ease-out;
  }
  .field-wrap:focus-within {
    box-shadow: inset 0 0 0 2px var(--md-sys-color-primary);
  }
  .field {
    flex: 1;
    min-height: 48px;
    max-height: 200px;
    field-sizing: content;
    padding: 13px 18px;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    color: var(--md-sys-color-on-surface);
    font: inherit;
    font-size: 14.5px;
    line-height: 22px;
    user-select: text;
  }
  .field::placeholder {
    color: var(--md-sys-color-on-surface-variant);
  }
  .field:focus-visible {
    outline: none;
  }
  .send {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 48px;
    height: 48px;
    border: none;
    border-radius: 24px;
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface-variant);
    cursor: default;
    transition:
      border-radius var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .send-ready {
    border-radius: 16px;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    cursor: pointer;
  }
  .send-ready:hover {
    border-radius: 24px;
  }
  .send-ready:active {
    transform: scale(0.92);
  }
</style>
