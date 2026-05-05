<!--
  Single chat message bubble — text, images, file cards, star, copy, time.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { showInExplorer, openFile, downloadFile, isMobile, getVideoSrc, revokeBlobUrl } from "$lib/api/bridge";
  import type { MessageAttachment, MessageEntry } from "$lib/state/app-state.svelte";
  import { onDestroy } from "svelte";

  interface Props {
    msg: MessageEntry;
    position: "solo" | "first" | "middle" | "last";
    isCopied: boolean;
    viewAll: boolean;
    thumbCache: Record<string, string>;
    oncopy: (id: string, text: string) => void;
    onlightbox: (path: string, name: string) => void;
    onfilepreview: (path: string) => void;
    onsnackbar?: (msg: string) => void;
  }

  let { msg, position, isCopied, viewAll, thumbCache, oncopy, onlightbox, onfilepreview, onsnackbar }: Props = $props();

  const app = getAppState();
  const mobile = isMobile();

  let downloading = $state<Record<string, boolean>>({});
  let videoUrls = $state<Record<string, string>>({});
  let videoEls: Record<string, HTMLVideoElement> = {};
  let videoProgress = $state<Record<string, number>>({});
  let videoDuration = $state<Record<string, number>>({});
  let videoMuted = $state<Record<string, boolean>>({});

  onDestroy(() => {
    for (const path of Object.keys(videoEls)) {
      const el = videoEls[path];
      if (el) { el.pause(); el.src = ""; }
    }
    for (const url of Object.values(videoUrls)) {
      revokeBlobUrl(url);
    }
  });

  // Resolve video asset URLs on mount
  $effect(() => {
    for (const att of msg.attachments ?? []) {
      if (att.type === "video" && att.path && !(att.path in videoUrls)) {
        getVideoSrc(att.path).then(url => {
          if (url) videoUrls = { ...videoUrls, [att.path]: url };
        });
      }
    }
  });

  function handleVideoEnter(path: string) {
    const el = videoEls[path];
    if (el) { el.muted = videoMuted[path] !== false; el.play().catch(() => {}); }
  }
  function handleVideoLeave(path: string) {
    const el = videoEls[path];
    if (el) { el.pause(); }
  }
  function handleVideoTimeUpdate(path: string) {
    const el = videoEls[path];
    if (el) videoProgress = { ...videoProgress, [path]: el.currentTime };
  }
  function handleVideoMeta(path: string) {
    const el = videoEls[path];
    if (el) videoDuration = { ...videoDuration, [path]: el.duration };
  }
  function toggleVideoMute(e: MouseEvent, path: string) {
    e.stopPropagation();
    const isMuted = videoMuted[path] !== false;
    videoMuted = { ...videoMuted, [path]: !isMuted };
    const el = videoEls[path];
    if (el) el.muted = !isMuted;
  }
  function handleVideoSeek(e: MouseEvent, path: string) {
    e.stopPropagation();
    const bar = (e.currentTarget as HTMLElement);
    const rect = bar.getBoundingClientRect();
    const pct = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const el = videoEls[path];
    if (el && el.duration) el.currentTime = pct * el.duration;
  }

  async function handleDownload(path: string, name: string) {
    if (downloading[path]) return;
    downloading = { ...downloading, [path]: true };
    try {
      const savedTo = await downloadFile(path);
      const folder = savedTo.includes("/Pictures/") ? "Pictures/LanDrop" : "Downloads";
      onsnackbar?.(`Saved ${name} → ${folder}`);
    } catch (e: any) {
      const detail = e?.message ?? String(e);
      onsnackbar?.(`Save failed: ${detail}`);
    }
    downloading = { ...downloading, [path]: false };
  }

  let isSent = $derived(msg.direction === "sent");
  let hasAttachments = $derived(msg.attachments && msg.attachments.length > 0);
  let media = $derived(msg.attachments?.filter(a => a.type === "image" || a.type === "video") ?? []);
  let files = $derived(msg.attachments?.filter(a => a.type === "file" || a.type === "folder") ?? []);

  function getPeerName(peerId: string): string {
    return app.devices.find(d => d.id === peerId)?.alias ?? "Unknown";
  }

  function formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  function getFolderPreviewItems(file: MessageAttachment) {
    return (file.children ?? []).slice(0, 3);
  }

  // Right-click toggles text-selection mode on this bubble
  let selectMode = $state(false);
  function handleContextMenu(e: MouseEvent) {
    if (!msg.text) return;
    e.preventDefault();
    selectMode = !selectMode;
  }
  function handleBubbleClick() {
    if (selectMode) return; // don't copy-all in select mode — let user select text
    if (msg.text) oncopy(msg.id, msg.text);
  }
</script>

<div
  class="msg-row"
  class:msg-row-sent={isSent}
  class:msg-row-received={!isSent}
  class:msg-group-break={position === "solo" || position === "first"}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="bubble"
    class:bubble-sent={isSent}
    class:bubble-received={!isSent}
    class:bubble-solo={position === "solo"}
    class:bubble-first={position === "first"}
    class:bubble-middle={position === "middle"}
    class:bubble-last={position === "last"}
    class:bubble-copied={isCopied}
    class:bubble-media={hasAttachments && !msg.text}
    class:bubble-select-mode={selectMode}
    onclick={handleBubbleClick}
    oncontextmenu={handleContextMenu}
    title={msg.text ? (selectMode ? "Selection mode — drag to select text, right-click to exit" : "Click to copy · Right-click to select text") : ""}
  >
    {#if viewAll}
      <div class="bubble-contact">{getPeerName(msg.peerId)}</div>
    {/if}

    {#if media.length > 0}
      <div class="att-images" class:att-grid={media.length > 1}>
        {#each media as item}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="att-img-wrap"
            onclick={(e) => { e.stopPropagation(); onlightbox(item.path, item.name); }}
            onmouseenter={() => item.type === "video" && handleVideoEnter(item.path)}
            onmouseleave={() => item.type === "video" && handleVideoLeave(item.path)}
          >
            {#if item.type === "video"}
              {#if videoUrls[item.path]}
                <!-- svelte-ignore a11y_media_has_caption -->
                <video
                  bind:this={videoEls[item.path]}
                  src="{videoUrls[item.path]}#t=0.1"
                  class="att-img"
                  preload="metadata"
                  muted
                  loop
                  playsinline
                  ontimeupdate={() => handleVideoTimeUpdate(item.path)}
                  onloadedmetadata={() => handleVideoMeta(item.path)}
                ></video>
                <div class="att-video-badge"><Icon name="play_arrow" size={16} /></div>
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="att-video-bar" onclick={(e) => handleVideoSeek(e, item.path)}>
                  <div class="att-video-bar-fill" style="width: {videoDuration[item.path] ? (videoProgress[item.path] ?? 0) / videoDuration[item.path] * 100 : 0}%"></div>
                </div>
                <button class="att-video-mute" onclick={(e) => toggleVideoMute(e, item.path)}>
                  <Icon name={videoMuted[item.path] !== false ? "volume_off" : "volume_up"} size={14} />
                </button>
              {:else}
                <div class="att-img-placeholder"><Icon name="videocam" size={24} /></div>
              {/if}
            {:else if thumbCache[item.path] && thumbCache[item.path] !== ""}
              <img src={thumbCache[item.path]} alt={item.name} class="att-img" />
            {:else}
              <div class="att-img-placeholder"><Icon name="image" size={24} /></div>
            {/if}
            {#if mobile && !isSent}
              <button class="att-img-download" onclick={(e) => { e.stopPropagation(); handleDownload(item.path, item.name); }} title="Save" disabled={downloading[item.path]}>
                <Icon name={downloading[item.path] ? "hourglass_empty" : "download"} size={16} />
              </button>
            {:else if !mobile}
              <button class="att-open-folder" onclick={(e) => { e.stopPropagation(); showInExplorer(item.path); }} title="Show in folder">
                <Icon name="open_in_new" size={14} />
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if files.length > 0}
      <div class="att-files">
        {#each files as file}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          {#if file.type === "folder"}
            <div class="att-file-card" onclick={(e) => { e.stopPropagation(); openFile(file.path); }} title="Open folder">
              <Icon name="folder" size={18} />
              <span class="att-file-name">{file.name}</span>
              {#if file.size}<span class="att-file-size">{file.size}</span>{/if}
              <span class="att-file-badge">{file.fileCount} {file.fileCount === 1 ? 'FILE' : 'FILES'}</span>
              {#if file.children?.length}
                <div class="att-folder-preview" aria-hidden="true">
                  {#each getFolderPreviewItems(file) as child}
                    <div class="att-folder-preview-item">
                      {#if child.type === "image" && thumbCache[child.path]}
                        <img src={thumbCache[child.path]} alt="" class="att-folder-preview-thumb" />
                      {:else if child.type === "video"}
                        <span class="att-folder-preview-icon"><Icon name="play_arrow" size={14} /></span>
                      {:else}
                        <span class="att-folder-preview-icon"><Icon name="description" size={14} /></span>
                      {/if}
                    </div>
                  {/each}
                  {#if file.children.length > 3}
                    <span class="att-folder-preview-more">+{file.children.length - 3}</span>
                  {/if}
                </div>
              {/if}
            </div>
          {:else}
            {@const ext = file.name.split('.').pop()?.toUpperCase() ?? 'FILE'}
            <div class="att-file-card" onclick={(e) => { e.stopPropagation(); mobile ? openFile(file.path) : onfilepreview(file.path); }} title={mobile ? "Open" : "Preview file"}>
              <span class="att-file-name">{file.name}</span>
              {#if file.size}<span class="att-file-size">{file.size}</span>{/if}
              <span class="att-file-badge">{ext}</span>
              {#if mobile && !isSent}
                <button class="att-file-download" onclick={(e) => { e.stopPropagation(); handleDownload(file.path, file.name); }} title="Save" disabled={downloading[file.path]}>
                  <Icon name={downloading[file.path] ? "hourglass_empty" : "download"} size={16} />
                </button>
              {:else if !mobile}
                <button class="att-file-open-folder" onclick={(e) => { e.stopPropagation(); showInExplorer(file.path); }} title="Show in folder">
                  <Icon name="open_in_new" size={12} />
                </button>
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    {/if}

    {#if msg.text}
      <span class="bubble-text">{msg.text}</span>
    {/if}

    <span class="bubble-time-spacer"></span>
    <span class="bubble-meta">
      {#if isCopied}
        <span class="bubble-copied-badge"><Icon name="check" size={10} /> Copied</span>
      {:else}
        <button
          class="star-btn"
          class:starred={msg.starred}
          onclick={(e) => { e.stopPropagation(); app.toggleStar(msg.id); }}
          title={msg.starred ? "Unsave" : "Save"}
        >
          <Icon name={msg.starred ? "star" : "star_border"} size={11} />
        </button>
        <span class="bubble-time">{formatTime(msg.timestamp)}</span>
      {/if}
    </span>
  </div>
</div>

<style>
  .msg-row {
    display: flex;
    margin-top: 2px;
  }
  .msg-row-sent { justify-content: flex-end; }
  .msg-row-received { justify-content: flex-start; }
  .msg-group-break { margin-top: 8px; }
  .msg-row:first-child { margin-top: 0; }

  .bubble {
    position: relative;
    max-width: clamp(140px, 78%, 400px);
    width: fit-content;
    padding: 6px 10px 4px;
    cursor: pointer;
    overflow-wrap: break-word;
    word-break: break-word;
    animation: bubble-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  .bubble-media { padding: 4px 4px 4px; }
  @keyframes bubble-in {
    from { opacity: 0; transform: translateY(3px) scale(0.98); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .bubble-sent {
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .bubble-received {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }

  .bubble-sent.bubble-solo   { border-radius: 18px 18px 4px 18px; }
  .bubble-sent.bubble-first  { border-radius: 18px 18px 4px 18px; }
  .bubble-sent.bubble-middle { border-radius: 18px 4px 4px 18px; }
  .bubble-sent.bubble-last   { border-radius: 18px 4px 18px 18px; }

  .bubble-received.bubble-solo   { border-radius: 18px 18px 18px 4px; }
  .bubble-received.bubble-first  { border-radius: 18px 18px 18px 4px; }
  .bubble-received.bubble-middle { border-radius: 4px 18px 18px 4px; }
  .bubble-received.bubble-last   { border-radius: 4px 18px 18px 18px; }

  .bubble::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--md-sys-color-on-surface);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .bubble:hover::after { opacity: 0.05; }
  .bubble:active::after { opacity: 0.08; }

  .bubble-copied { animation: bubble-flash var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects); }
  @keyframes bubble-flash { from { opacity: 0.7; } to { opacity: 1; } }

  .bubble-select-mode {
    cursor: text !important;
    outline: 2px solid var(--md-sys-color-primary);
    outline-offset: 1px;
  }
  .bubble-select-mode .bubble-text { cursor: text; }

  .bubble-contact { font-size: 10px; font-weight: 500; color: var(--md-sys-color-primary); margin-bottom: 1px; }
  .bubble-text {
    font-family: var(--md-sys-typescale-body-small-font, "Roboto Mono", monospace);
    font-size: 13px;
    line-height: 1.4;
    white-space: pre-wrap;
    user-select: text;
  }
  .bubble-time-spacer { display: inline-block; width: 58px; height: 1px; }
  .bubble-meta { position: absolute; right: 8px; bottom: 3px; display: flex; align-items: center; gap: 3px; }
  .bubble-time { font-size: 10px; line-height: 1; opacity: 0.45; white-space: nowrap; }
  .bubble-copied-badge {
    display: flex; align-items: center; gap: 2px; font-size: 10px; font-weight: 500;
    color: var(--md-sys-color-primary);
    animation: badge-pop var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes badge-pop { from { transform: scale(0.8); opacity: 0; } to { transform: scale(1); opacity: 1; } }

  .star-btn {
    display: flex; align-items: center; background: transparent; border: none;
    cursor: pointer; padding: 0; color: var(--md-sys-color-outline); opacity: 0;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects), color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .star-btn.starred { opacity: 1; color: var(--md-sys-color-primary); }
  .bubble:hover .star-btn { opacity: 0.7; }
  .bubble:hover .star-btn.starred { opacity: 1; }

  .att-images { margin-bottom: 4px; border-radius: 12px; overflow: hidden; }
  .att-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 3px; }
  .att-img-wrap { position: relative; background: var(--md-sys-color-surface-container); min-height: 60px; }
  .att-img { display: block; width: 100%; max-height: 200px; object-fit: cover; cursor: pointer; }
  .att-img-placeholder {
    display: flex; align-items: center; justify-content: center;
    width: 100%; min-height: 80px;
    color: var(--md-sys-color-on-surface-variant);
    animation: shimmer 1.5s cubic-bezier(0.2, 0.0, 0, 1.0) infinite alternate;
  }
  .att-grid .att-img { aspect-ratio: 1; max-height: none; }
  .att-img-download {
    position: absolute;
    bottom: 6px;
    right: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    cursor: pointer;
    z-index: 2;
    backdrop-filter: blur(4px);
  }
  .att-img-download:active { background: rgba(0, 0, 0, 0.75); }
  .att-img-download:disabled { opacity: 0.5; }

  .att-open-folder {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    cursor: pointer;
    z-index: 3;
    opacity: 0;
    backdrop-filter: blur(4px);
    transition: opacity 0.2s;
  }
  .att-img-wrap:hover .att-open-folder { opacity: 1; }
  .att-open-folder:active { background: rgba(0, 0, 0, 0.75); }

  .att-video-badge {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    pointer-events: none;
    backdrop-filter: blur(4px);
    transition: opacity 0.2s;
  }
  .att-img-wrap:hover .att-video-badge { opacity: 0; }
  .att-video-mute {
    position: absolute;
    bottom: 10px;
    right: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    cursor: pointer;
    z-index: 3;
    opacity: 0;
    backdrop-filter: blur(4px);
    transition: opacity 0.2s;
  }
  .att-img-wrap:hover .att-video-mute { opacity: 1; }
  .att-video-mute:active { background: rgba(0, 0, 0, 0.75); }
  .att-video-bar {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 4px;
    background: rgba(255, 255, 255, 0.2);
    cursor: pointer;
    z-index: 3;
    opacity: 0;
    transition: opacity 0.2s, height 0.15s;
  }
  .att-img-wrap:hover .att-video-bar { opacity: 1; height: 6px; }
  .att-video-bar-fill {
    height: 100%;
    background: var(--md-sys-color-primary, #bb86fc);
    border-radius: 0 1px 1px 0;
    transition: width 0.1s linear;
  }

  .att-file-download {
    position: absolute;
    top: 8px;
    right: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: none;
    background: color-mix(in srgb, var(--md-sys-color-primary) 15%, transparent);
    color: var(--md-sys-color-primary);
    cursor: pointer;
    z-index: 2;
  }
  .att-file-download:active { background: color-mix(in srgb, var(--md-sys-color-primary) 30%, transparent); }
  .att-file-download:disabled { opacity: 0.5; }
  .att-file-open-folder {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: none;
    background: color-mix(in srgb, var(--md-sys-color-primary) 15%, transparent);
    color: var(--md-sys-color-primary);
    cursor: pointer;
    z-index: 2;
    opacity: 0;
    transition: opacity 0.2s;
  }
  .att-file-card:hover .att-file-open-folder { opacity: 1; }
  .att-file-open-folder:active { background: color-mix(in srgb, var(--md-sys-color-primary) 30%, transparent); }

  .att-files { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 4px; }
  .att-file-card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 140px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 25%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 4%, transparent);
    cursor: pointer;
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .att-file-card:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent);
  }
  .att-file-name {
    font-size: 12px;
    font-weight: 600;
    line-height: 1.3;
    word-break: break-word;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .att-file-size {
    font-size: 11px;
    opacity: 0.5;
  }
  .att-file-badge {
    display: inline-block;
    margin-top: 6px;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    width: fit-content;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    color: var(--md-sys-color-on-surface-variant);
  }
  .att-folder-preview {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 8px;
    min-height: 22px;
  }
  .att-folder-preview-item {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    overflow: hidden;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .att-folder-preview-thumb {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .att-folder-preview-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--md-sys-color-on-surface-variant);
  }
  .att-folder-preview-more {
    font-size: 10px;
    font-weight: 600;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.75;
    margin-left: 2px;
  }
</style>
