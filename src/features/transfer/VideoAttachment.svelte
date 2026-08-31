<!--
  Hover-play video tile used in chat bubbles.

  Owns the whole video state machine (blob URL lifetime, play/pause on hover,
  seek bar, mute) so MessageBubble no longer keeps six parallel records, and so
  the blob lifecycle (acquire on load, release on unmount) lives in one place.
  Visibility of the badge/bar/mute is driven by an explicit `hovered` flag
  rather than a `:hover` descendant selector, so the tile stays self-contained.
-->
<script lang="ts">
  import { onDestroy, type Snippet } from "svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import { getVideoSrc, revokeBlobUrl } from "$lib/api/bridge";
  import { seekRatio } from "$lib/utils/video-utils";

  interface Props {
    path: string;
    name: string;
    /** Square variant used when the bubble shows a multi-item media grid. */
    grid?: boolean;
    onopen: () => void;
    /** Platform action button (save on Android, reveal on desktop). */
    action?: Snippet;
  }

  let { path, name, grid = false, onopen, action }: Props = $props();

  let videoEl = $state<HTMLVideoElement | null>(null);
  let url = $state<string | null>(null);
  let failed = $state(false);
  let hovered = $state(false);
  let muted = $state(true);
  let progress = $state(0);
  let duration = $state(0);

  // Plain (non-reactive) bookkeeping so the loader effect depends on `path` only.
  let loadedPath = "";
  let loadedUrl: string | null = null;
  let destroyed = false;

  $effect(() => {
    const wanted = path;
    if (!wanted || wanted === loadedPath) return;
    loadedPath = wanted;

    if (loadedUrl) {
      revokeBlobUrl(loadedUrl);
      loadedUrl = null;
    }
    url = null;
    failed = false;

    getVideoSrc(wanted)
      .then((next) => {
        // Unmounted, or the tile moved on to another path: release immediately.
        if (destroyed || loadedPath !== wanted) {
          if (next) revokeBlobUrl(next);
          return;
        }
        if (next) {
          loadedUrl = next;
          url = next;
        } else {
          failed = true;
        }
      })
      .catch(() => {
        if (!destroyed && loadedPath === wanted) failed = true;
      });
  });

  onDestroy(() => {
    destroyed = true;
    if (videoEl) {
      videoEl.pause();
      videoEl.src = "";
    }
    if (loadedUrl) revokeBlobUrl(loadedUrl);
  });

  function handleEnter() {
    hovered = true;
    if (!videoEl) return;
    videoEl.muted = muted;
    videoEl.play().catch(() => {});
  }

  function handleLeave() {
    hovered = false;
    videoEl?.pause();
  }

  function toggleMute(e: MouseEvent) {
    e.stopPropagation();
    muted = !muted;
    if (videoEl) videoEl.muted = muted;
  }

  function handleSeek(e: MouseEvent) {
    e.stopPropagation();
    if (!videoEl?.duration) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    videoEl.currentTime = seekRatio(e.clientX, rect) * videoEl.duration;
  }

  const fillPct = $derived(duration > 0 ? (progress / duration) * 100 : 0);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="att-video-tile"
  class:att-video-grid={grid}
  class:att-video-hovered={hovered}
  onclick={(e) => {
    e.stopPropagation();
    onopen();
  }}
  onmouseenter={handleEnter}
  onmouseleave={handleLeave}
>
  {#if url}
    <video
      bind:this={videoEl}
      src="{url}#t=0.1"
      class="att-video"
      preload="metadata"
      muted
      loop
      playsinline
      ontimeupdate={() => {
        if (videoEl) progress = videoEl.currentTime;
      }}
      onloadedmetadata={() => {
        if (videoEl) duration = videoEl.duration;
      }}
    ></video>
    <div class="att-video-badge"><Icon name="play_arrow" size={16} /></div>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="att-video-bar" onclick={handleSeek}>
      <div class="att-video-bar-fill" style="width: {fillPct}%"></div>
    </div>
    <button
      class="att-video-mute"
      onclick={toggleMute}
      aria-label={muted ? `Unmute ${name}` : `Mute ${name}`}
    >
      <Icon name={muted ? "volume_off" : "volume_up"} size={14} />
    </button>
  {:else}
    <div
      class="att-video-placeholder"
      class:att-video-skeleton={!failed}
      class:att-media-unavailable={failed}
      aria-hidden="true"
    >
      <Icon name={failed ? "videocam_off" : "videocam"} size={24} />
    </div>
  {/if}
  {@render action?.()}
</div>

<style>
  .att-video-tile {
    position: relative;
    background: var(--md-sys-color-surface-container);
    overflow: hidden;
    width: 100%;
    aspect-ratio: 3 / 4;
    cursor: pointer;
  }
  .att-video-grid {
    aspect-ratio: 1;
  }

  .att-video {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    cursor: pointer;
  }

  .att-video-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    min-height: 80px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .att-video-skeleton {
    background:
      linear-gradient(
        90deg,
        transparent,
        color-mix(in srgb, var(--md-sys-color-on-surface) 7%, transparent),
        transparent
      ),
      var(--md-sys-color-surface-container-high);
    background-size:
      220% 100%,
      auto;
    animation: video-skeleton 1.4s cubic-bezier(0.2, 0, 0, 1) infinite;
  }
  .att-media-unavailable {
    background: var(--md-sys-color-surface-container-high);
    animation: none;
    opacity: 0.72;
  }
  @keyframes video-skeleton {
    from {
      background-position:
        120% 0,
        0 0;
      opacity: 0.72;
    }
    to {
      background-position:
        -120% 0,
        0 0;
      opacity: 1;
    }
  }

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
  .att-video-hovered .att-video-badge {
    opacity: 0;
  }

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
  .att-video-hovered .att-video-mute {
    opacity: 1;
  }
  .att-video-mute:active {
    background: rgba(0, 0, 0, 0.75);
  }

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
    transition:
      opacity 0.2s,
      height 0.15s;
  }
  .att-video-hovered .att-video-bar {
    opacity: 1;
    height: 6px;
  }
  .att-video-bar-fill {
    height: 100%;
    background: var(--md-sys-color-primary, #bb86fc);
    border-radius: 0 1px 1px 0;
    transition: width 0.1s linear;
  }

  /* The action button is provided by the parent as a snippet, so it carries
     the parent's style scope — reveal it from here on hover. */
  .att-video-hovered :global(.att-tile-action) {
    opacity: 1;
  }
</style>
