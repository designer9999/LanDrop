<!--
  Bottom composer — FAB attach menu, file/image previews, textarea, send button.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import FileCard from "./FileCard.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import {
    saveClipboardImage,
    getFileInfo,
    getClipboardFiles,
    getVideoSrc,
    revokeBlobUrl,
  } from "$lib/api/bridge";
  import {
    fileExtensionLabel,
    fileNameFromPath,
    fileSizeStr,
    isImage,
    isVideo,
  } from "$lib/utils/file-utils";
  import { onDestroy, onMount } from "svelte";
  import type { SvelteMap } from "svelte/reactivity";

  interface Props {
    peerName?: string;
    thumbCache: SvelteMap<string, string>;
    onpickfiles: () => void;
    onpickfolder: () => void;
    onsend: () => void;
    onlightbox: (path: string, name: string) => void;
    onfilepreview: (path: string) => void;
    onaddpaths: (paths: string[]) => void;
  }

  let {
    peerName,
    thumbCache,
    onpickfiles,
    onpickfolder,
    onsend,
    onlightbox,
    onfilepreview,
    onaddpaths,
  }: Props = $props();

  const app = getAppState();

  let composerEl: HTMLTextAreaElement | undefined = $state();
  let fabMenuOpen = $state(false);

  const canSend = $derived(
    app.activeDeviceOnline && !app.transferActive && (app.hasFiles || !!app.sendTextContent.trim()),
  );
  const isMediaFile = (type: string) => isImage(type) || isVideo(type);
  const imageFiles = $derived(app.files.filter((f) => f.info && isMediaFile(f.info.type)));
  const otherFiles = $derived(app.files.filter((f) => !f.info || !isMediaFile(f.info.type)));

  let videoUrls = $state<Record<string, string>>({});
  // Plain Set on purpose: an in-flight guard, never rendered.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const _videoLoading = new Set<string>();
  let destroyed = false;

  $effect(() => {
    const currentVideoPaths = new Set(
      imageFiles.filter((f) => f.info && isVideo(f.info.type)).map((f) => f.path),
    );

    // Release URLs whose attachment left the composer (removed or sent)
    const stale = Object.keys(videoUrls).filter((p) => !currentVideoPaths.has(p));
    if (stale.length > 0) {
      const next = { ...videoUrls };
      for (const p of stale) {
        revokeBlobUrl(next[p]);
        delete next[p];
      }
      videoUrls = next;
    }

    for (const f of imageFiles) {
      if (f.info && isVideo(f.info.type) && !(f.path in videoUrls) && !_videoLoading.has(f.path)) {
        _videoLoading.add(f.path);
        getVideoSrc(f.path)
          .then((url) => {
            _videoLoading.delete(f.path);
            if (destroyed) {
              if (url) revokeBlobUrl(url);
              return;
            }
            if (url) videoUrls = { ...videoUrls, [f.path]: url };
          })
          .catch(() => {
            _videoLoading.delete(f.path);
          });
      }
    }
  });

  onDestroy(() => {
    destroyed = true;
    for (const url of Object.values(videoUrls)) {
      revokeBlobUrl(url);
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      if (canSend) onsend();
    }
  }

  async function handlePaste(e: ClipboardEvent) {
    if (!e.clipboardData) return;

    const items = e.clipboardData.items;
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const blob = item.getAsFile();
        if (!blob) continue;
        const reader = new FileReader();
        reader.onload = async () => {
          const dataUrl = reader.result as string;
          const base64 = dataUrl.split(",")[1];
          if (!base64) return;
          const path = await saveClipboardImage(base64, item.type);
          const info = await getFileInfo(path);
          app.addFile(path, info);
        };
        reader.readAsDataURL(blob);
        return;
      }
    }

    try {
      const files = await getClipboardFiles();
      if (files.length > 0) {
        e.preventDefault();
        onaddpaths(files);
      }
    } catch {
      // No file paths in clipboard, let normal paste happen
    }
  }

  onMount(() => {
    function handleWindowClick() {
      fabMenuOpen = false;
    }
    window.addEventListener("click", handleWindowClick);
    return () => window.removeEventListener("click", handleWindowClick);
  });
</script>

<div class="composer">
  {#if fabMenuOpen}
    <div class="fab-menu-items">
      <button
        class="fab-menu-item"
        style="animation-delay: 0ms;"
        onclick={(e) => {
          e.stopPropagation();
          fabMenuOpen = false;
          onpickfiles();
        }}
      >
        <Icon name="attach_file" size={18} />
        <span class="text-sm font-medium">Files</span>
      </button>
      <button
        class="fab-menu-item"
        style="animation-delay: 40ms;"
        onclick={(e) => {
          e.stopPropagation();
          fabMenuOpen = false;
          onpickfolder();
        }}
      >
        <Icon name="folder" size={18} />
        <span class="text-sm font-medium">Folder</span>
      </button>
    </div>
  {/if}

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="composer-box"
    onclick={(e) => {
      if ((e.target as HTMLElement).closest("button, .composer-img")) return;
      composerEl?.focus();
    }}
  >
    {#if app.hasFiles}
      <div class="composer-attachments">
        {#each imageFiles as file (file.path)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="composer-img"
            onclick={() => onlightbox(file.path, file.info?.name ?? "image")}
          >
            {#if file.info && isVideo(file.info.type)}
              {#if videoUrls[file.path]}
                <video
                  src="{videoUrls[file.path]}#t=0.1"
                  class="composer-img-preview"
                  preload="metadata"
                  muted
                  playsinline
                ></video>
              {:else}
                <div class="composer-img-loading"><div class="composer-img-shimmer"></div></div>
              {/if}
            {:else if thumbCache.get(file.path)}
              <img
                src={thumbCache.get(file.path)}
                alt={file.info?.name ?? "image"}
                class="composer-img-preview"
              />
            {:else}
              <div class="composer-img-loading"><div class="composer-img-shimmer"></div></div>
            {/if}
            {#if !app.transferActive}
              <button
                class="composer-img-remove"
                onclick={(e) => {
                  e.stopPropagation();
                  app.removeFile(file.path);
                }}
              >
                <Icon name="close" size={14} />
              </button>
            {/if}
          </div>
        {/each}
        {#each otherFiles as file (file.path)}
          {@const name = file.info?.name ?? fileNameFromPath(file.path, "file")}
          <FileCard
            compact
            icon={file.info?.type === "folder" ? "folder" : "description"}
            {name}
            size={file.info?.size_bytes ? fileSizeStr(file.info.size_bytes) : undefined}
            badge={file.info?.type === "folder" ? "FOLDER" : fileExtensionLabel(name)}
            onclick={() => onfilepreview(file.path)}
          >
            {#snippet action()}
              {#if !app.transferActive}
                <button
                  class="composer-file-remove file-card-action"
                  onclick={(e) => {
                    e.stopPropagation();
                    app.removeFile(file.path);
                  }}
                  aria-label="Remove {name}"
                >
                  <Icon name="close" size={14} />
                </button>
              {/if}
            {/snippet}
          </FileCard>
        {/each}
      </div>
    {/if}
    <textarea
      bind:this={composerEl}
      class="composer-textarea"
      placeholder={peerName ? `Message ${peerName}...` : "Type a message..."}
      rows={1}
      bind:value={app.sendTextContent}
      onkeydown={handleKeydown}
      onpaste={handlePaste}></textarea>

    <div class="composer-actions">
      <button
        class="fab-btn"
        style="
          transform: rotate({fabMenuOpen ? '45deg' : '0deg'});
          border-radius: {fabMenuOpen ? '50%' : '12px'};
          background-color: var({fabMenuOpen ? '--color-primary' : '--color-primary-container'});
          color: var({fabMenuOpen ? '--color-on-primary' : '--color-on-primary-container'});
          transition:
            transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
            border-radius var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
            background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
            color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
        onclick={(e) => {
          e.stopPropagation();
          fabMenuOpen = !fabMenuOpen;
        }}
        title={fabMenuOpen ? "Close" : "Attach"}
      >
        <Icon name="add" size={20} />
      </button>

      <span class="flex-1"></span>

      <button
        class="send-btn"
        class:send-active={canSend}
        disabled={!canSend}
        onclick={onsend}
        title="Send"
      >
        <Icon name="arrow_upward" size={20} />
      </button>
    </div>
  </div>
</div>

<style>
  .composer {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    max-height: 70%;
    padding: 8px 12px 12px;
    display: flex;
    flex-direction: column;
    pointer-events: none;
    z-index: 10;
  }

  .composer-box {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    border-radius: 12px;
    border: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface);
    transition: border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
    cursor: text;
    overflow: hidden;
    pointer-events: auto;
  }
  .composer-box:focus-within {
    border-color: var(--md-sys-color-primary);
  }

  .composer-actions {
    display: flex;
    align-items: center;
    padding: 4px 6px 6px;
    flex-shrink: 0;
  }

  .composer-attachments {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 8px 12px 4px;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-outline) 30%, transparent) transparent;
  }

  /* ── Image previews ── */
  .composer-img {
    position: relative;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    overflow: hidden;
    cursor: pointer;
    flex-shrink: 0;
  }
  .composer-img::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .composer-img:hover::after {
    opacity: 0.08;
  }

  .composer-img-preview {
    display: block;
    width: 56px;
    height: 56px;
    object-fit: cover;
    border-radius: 8px;
  }

  .composer-img-loading {
    width: 56px;
    height: 56px;
    border-radius: 8px;
    background: var(--md-sys-color-surface-container);
    overflow: hidden;
    position: relative;
  }
  .composer-img-shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent) 50%,
      transparent 100%
    );
    animation: shimmer-slide 1.5s cubic-bezier(0.2, 0, 0, 1) infinite;
  }
  @keyframes shimmer-slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(100%);
    }
  }

  .composer-img-remove {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
    padding: 0;
    z-index: 1;
  }
  .composer-img:hover .composer-img-remove {
    opacity: 1;
  }

  /* ── File cards (chrome lives in FileCard.svelte) ── */
  .composer-file-remove {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: none;
    background: var(--md-sys-color-surface-container-highest);
    color: var(--md-sys-color-on-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
    padding: 0;
    z-index: 1;
  }
  .composer-file-remove:hover {
    color: var(--md-sys-color-error);
  }

  /* ── Textarea ── */
  .composer-textarea {
    display: block;
    width: 100%;
    box-sizing: border-box;
    border: none;
    outline: none;
    background: transparent;
    color: var(--md-sys-color-on-surface);
    font-family: var(--font-plain);
    font-size: 14px;
    line-height: 20px;
    padding: 10px 16px 4px;
    resize: none;
    min-height: 20px;
    max-height: 120px;
    overflow-y: auto;
    field-sizing: content;
    scrollbar-width: thin;
    word-break: break-word;
    overflow-wrap: break-word;
    flex-shrink: 1;
  }
  .composer-textarea::placeholder {
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
  }

  /* ── FAB button ── */
  .fab-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: none;
    cursor: pointer;
    position: relative;
    overflow: hidden;
    box-shadow: var(--shadow-level3);
  }
  .fab-btn::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-primary-container);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .fab-btn:hover::after {
    opacity: 0.08;
  }
  .fab-btn:active::after {
    opacity: 0.1;
  }
  .fab-btn:hover {
    box-shadow: var(--shadow-level4);
  }

  /* ── Send button ── */
  .send-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    border: none;
    flex-shrink: 0;
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    cursor: default;
    opacity: 0.4;
    box-shadow: var(--shadow-level3);
    position: relative;
    overflow: hidden;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .send-btn::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-primary-container);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .send-active {
    cursor: pointer;
    opacity: 1;
  }
  .send-btn:hover::after {
    opacity: 0.08;
  }
  .send-btn:active::after {
    opacity: 0.1;
  }

  /* ── FAB menu pills ── */
  .fab-menu-items {
    position: absolute;
    bottom: calc(100% + 4px);
    left: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 10;
    pointer-events: auto;
  }
  .fab-menu-item {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 40px;
    padding: 0 16px;
    border-radius: 9999px;
    border: none;
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
    box-shadow: var(--shadow-level3);
    cursor: pointer;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 500;
    position: relative;
    overflow: hidden;
    animation: fab-menu-enter var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  .fab-menu-item::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-primary-container);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .fab-menu-item:hover::after {
    opacity: 0.08;
  }
  .fab-menu-item:active::after {
    opacity: 0.1;
  }
  @keyframes fab-menu-enter {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.92);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>
