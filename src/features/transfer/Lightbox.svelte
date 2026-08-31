<!--
  Fullscreen image/video lightbox with header actions.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { showInExplorer, openFile, downloadFile, isMobile, getVideoSrc } from "$lib/api/bridge";
  import { isVideo } from "$lib/utils/file-utils";
  import { formatDuration } from "$lib/utils/video-utils";
  import { onMount, onDestroy } from "svelte";
  import { revokeBlobUrl } from "$lib/api/bridge";

  interface Props {
    src: string;
    name: string;
    path: string;
    loading: boolean;
    onclose: () => void;
    onsnackbar?: (msg: string) => void;
  }

  let { src, name, path, loading, onclose, onsnackbar }: Props = $props();
  const mobile = isMobile();
  let saving = $state(false);
  const isVid = $derived(isVideo(name));
  let videoSrc = $state<string | null>(null);
  let videoEl = $state<HTMLVideoElement | null>(null);
  let videoMuted = $state(false);
  let videoProgress = $state(0);
  let videoDuration = $state(0);
  let videoPlaying = $state(false);

  let destroyed = false;

  onMount(() => {
    if (isVid) {
      getVideoSrc(path).then((s) => {
        if (destroyed) {
          if (s) revokeBlobUrl(s);
          return;
        }
        videoSrc = s;
      });
    }
  });

  onDestroy(() => {
    destroyed = true;
    if (videoEl) {
      videoEl.pause();
      videoEl.src = "";
    }
    if (videoSrc) revokeBlobUrl(videoSrc);
  });

  async function handleDownload() {
    if (saving) return;
    saving = true;
    try {
      const savedTo = await downloadFile(path);
      const folder = savedTo.includes("/Pictures/") ? "Pictures/LanDrop" : "Downloads";
      onsnackbar?.(`Saved ${name} → ${folder}`);
    } catch {
      onsnackbar?.(`Failed to save ${name}`);
    }
    saving = false;
  }

  function togglePlayPause() {
    if (!videoEl) return;
    if (videoEl.paused) {
      videoEl.play().catch(() => {});
      videoPlaying = true;
    } else {
      videoEl.pause();
      videoPlaying = false;
    }
  }

  function handleTimeUpdate() {
    if (!videoEl) return;
    videoProgress = videoEl.currentTime;
    videoDuration = videoEl.duration || 0;
  }

  function handleSeek(e: Event) {
    if (!videoEl) return;
    videoEl.currentTime = Number((e.target as HTMLInputElement).value);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="lightbox" onclick={onclose}>
  <div class="lightbox-header">
    <span class="lightbox-name">{name}</span>
    <div class="lightbox-actions">
      {#if mobile}
        <button
          class="lightbox-btn"
          onclick={(e) => {
            e.stopPropagation();
            handleDownload();
          }}
          title="Save"
          disabled={saving}
        >
          <Icon name={saving ? "hourglass_empty" : "download"} size={18} />
        </button>
        <button
          class="lightbox-btn"
          onclick={(e) => {
            e.stopPropagation();
            openFile(path);
          }}
          title="Open"
        >
          <Icon name="open_in_new" size={18} />
        </button>
      {:else}
        <button
          class="lightbox-btn"
          onclick={(e) => {
            e.stopPropagation();
            showInExplorer(path);
          }}
          title="Open in folder"
        >
          <Icon name="folder_open" size={18} />
        </button>
      {/if}
      <button class="lightbox-close" onclick={onclose}>
        <Icon name="close" size={20} />
      </button>
    </div>
  </div>

  {#if isVid && videoSrc}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="lightbox-video-container" onclick={(e) => e.stopPropagation()}>
      <video
        bind:this={videoEl}
        src={videoSrc}
        class="lightbox-video"
        muted={videoMuted}
        playsinline
        ontimeupdate={handleTimeUpdate}
        onplay={() => (videoPlaying = true)}
        onpause={() => (videoPlaying = false)}
        onloadedmetadata={() => {
          if (videoEl) videoDuration = videoEl.duration;
        }}
        poster={src || undefined}
        autoplay
      ></video>
      <div class="lightbox-video-controls">
        <button class="lightbox-ctrl-btn" onclick={togglePlayPause}>
          <Icon name={videoPlaying ? "pause" : "play_arrow"} size={20} />
        </button>
        <span class="lightbox-video-time">{formatDuration(videoProgress)}</span>
        <input
          type="range"
          class="lightbox-video-seek"
          min="0"
          max={videoDuration}
          step="0.1"
          value={videoProgress}
          oninput={handleSeek}
        />
        <span class="lightbox-video-time">{formatDuration(videoDuration)}</span>
        <button
          class="lightbox-ctrl-btn"
          onclick={() => {
            videoMuted = !videoMuted;
            if (videoEl) videoEl.muted = videoMuted;
          }}
        >
          <Icon name={videoMuted ? "volume_off" : "volume_up"} size={18} />
        </button>
      </div>
    </div>
  {:else}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <img
      {src}
      alt={name}
      class="lightbox-img"
      class:lightbox-img-loading={loading}
      onclick={(e) => e.stopPropagation()}
    />
    {#if loading}
      <span class="lightbox-loading">Loading full image...</span>
    {/if}
  {/if}
</div>

<style>
  .lightbox {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.85);
    animation: overlay-in var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) both;
    cursor: pointer;
  }
  @keyframes overlay-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .lightbox-header {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: linear-gradient(rgba(0, 0, 0, 0.6), transparent);
    z-index: 2;
  }
  .lightbox-name {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.8);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .lightbox-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .lightbox-btn,
  .lightbox-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.15);
    color: #fff;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .lightbox-btn:hover,
  .lightbox-close:hover {
    background: rgba(255, 255, 255, 0.25);
  }
  .lightbox-img {
    max-width: 90%;
    max-height: 80%;
    object-fit: contain;
    border-radius: 8px;
    cursor: default;
    transition: opacity var(--md-spring-default-effects-dur) var(--md-spring-default-effects);
  }
  .lightbox-img-loading {
    opacity: 0.5;
    filter: blur(2px);
  }
  .lightbox-loading {
    margin-top: 8px;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
  }

  /* ── Video lightbox ── */
  .lightbox-video-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 90%;
    max-height: 80%;
    cursor: default;
  }
  .lightbox-video {
    max-width: 100%;
    max-height: calc(80vh - 48px);
    border-radius: 8px;
    object-fit: contain;
    background: #000;
  }
  .lightbox-video-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 4px 0;
  }
  .lightbox-ctrl-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.12);
    color: #fff;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s;
  }
  .lightbox-ctrl-btn:hover {
    background: rgba(255, 255, 255, 0.25);
  }
  .lightbox-video-time {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.7);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    min-width: 32px;
    text-align: center;
  }
  .lightbox-video-seek {
    flex: 1;
    height: 4px;
    appearance: none;
    background: rgba(255, 255, 255, 0.2);
    border-radius: 2px;
    outline: none;
    cursor: pointer;
  }
  .lightbox-video-seek::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--md-sys-color-primary, #bb86fc);
    cursor: pointer;
  }
</style>
