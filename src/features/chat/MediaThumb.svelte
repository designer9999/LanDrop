<!--
  Image or video tile. Loads only once scrolled near the viewport; images use
  the cached Rust thumbnail, videos show their first frame through the asset
  protocol (range requests — the file is never copied into memory).
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { getMediaUrl, getThumbnail } from "$lib/api/bridge";
  import type { MessageAttachment } from "$lib/state/model";
  import { formatDuration } from "$lib/utils/video-utils";

  interface Props {
    attachment: MessageAttachment;
    /** Larger thumbnail for a single image; small for grids and the composer tray. */
    large?: boolean;
    /** Text drawn over the tile (e.g. "+3" on the last grid tile). */
    overlay?: string;
    onopen?: () => void;
  }

  let { attachment, large = false, overlay, onopen }: Props = $props();

  let host: HTMLElement | undefined = $state();
  let visible = $state(false);
  let src = $state<string | null>(null);
  let failed = $state(false);
  let loaded = $state(false);
  let duration = $state(0);
  let aspect = $state<number | null>(null);

  const isVideo = $derived(attachment.type === "video");

  $effect(() => {
    if (!host) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          visible = true;
          observer.disconnect();
        }
      },
      { rootMargin: "600px 0px" },
    );
    observer.observe(host);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!visible) return;
    const path = attachment.path;
    let cancelled = false;
    failed = false;
    loaded = false;
    const load = isVideo ? getMediaUrl(path) : getThumbnail(path, large ? 640 : 320);
    void load.then((url) => {
      if (cancelled) return;
      src = url;
      failed = !url;
    });
    return () => {
      cancelled = true;
    };
  });
</script>

<button
  bind:this={host}
  class="thumb"
  class:thumb-large={large}
  class:thumb-loaded={loaded}
  style:aspect-ratio={large && aspect ? Math.min(Math.max(aspect, 0.6), 2.2) : null}
  onclick={(event) => {
    event.stopPropagation();
    onopen?.();
  }}
  aria-label="Open {attachment.name}"
  title={attachment.name}
>
  {#if failed}
    <span class="thumb-missing">
      <Icon name={isVideo ? "videocam_off" : "hide_image"} size={24} />
      <span>Not available</span>
    </span>
  {:else if src && isVideo}
    <video
      src="{src}#t=0.1"
      preload="metadata"
      muted
      playsinline
      onloadedmetadata={(event) => {
        const video = event.currentTarget;
        duration = video.duration;
        if (video.videoWidth && video.videoHeight) aspect = video.videoWidth / video.videoHeight;
        loaded = true;
      }}
      onerror={() => (failed = true)}
    ></video>
  {:else if src}
    <img
      {src}
      alt={attachment.name}
      decoding="async"
      draggable="false"
      onload={(event) => {
        const image = event.currentTarget as HTMLImageElement;
        if (image.naturalWidth && image.naturalHeight) {
          aspect = image.naturalWidth / image.naturalHeight;
        }
        loaded = true;
      }}
      onerror={() => (failed = true)}
    />
  {/if}
  {#if !loaded && !failed}
    <span class="thumb-shimmer"></span>
  {/if}
  {#if isVideo && !failed}
    <span class="thumb-play"><Icon name="play_arrow" size={large ? 28 : 22} /></span>
    {#if duration}
      <span class="thumb-duration">{formatDuration(duration)}</span>
    {/if}
  {/if}
  {#if overlay}
    <span class="thumb-overlay">{overlay}</span>
  {/if}
</button>

<style>
  .thumb {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
    min-height: 0;
    padding: 0;
    border: none;
    overflow: hidden;
    border-radius: inherit;
    background: var(--md-sys-color-surface-container-highest);
    cursor: zoom-in;
  }
  .thumb-large {
    height: auto;
    aspect-ratio: 4 / 3;
    max-height: 340px;
  }
  .thumb img,
  .thumb video {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    transition: opacity 180ms ease-out;
  }
  .thumb-loaded img,
  .thumb-loaded video {
    opacity: 1;
  }
  .thumb:focus-visible {
    outline: 3px solid var(--md-sys-color-primary);
    outline-offset: -3px;
  }
  .thumb-shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      100deg,
      transparent 20%,
      color-mix(in srgb, var(--md-sys-color-on-surface) 6%, transparent) 50%,
      transparent 80%
    );
    background-size: 200% 100%;
    animation: shimmer 1.2s linear infinite;
  }
  @keyframes shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }
  .thumb-missing {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
    cursor: default;
  }
  .thumb-play {
    position: absolute;
    top: 50%;
    left: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    transform: translate(-50%, -50%);
    background: color-mix(in srgb, var(--md-sys-color-scrim) 55%, transparent);
    color: white;
    pointer-events: none;
  }
  .thumb-duration {
    position: absolute;
    left: 8px;
    bottom: 8px;
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 60%, transparent);
    color: white;
    pointer-events: none;
  }
  .thumb-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    font-weight: 600;
    color: white;
    background: color-mix(in srgb, var(--md-sys-color-scrim) 50%, transparent);
    pointer-events: none;
  }
</style>
