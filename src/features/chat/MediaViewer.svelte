<!--
  Full-screen media viewer. Full-resolution images and streamed video from the
  asset protocol; ← → (or swipe) moves through the conversation's media;
  wheel / double-click zooms, drag pans; Esc closes.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import {
    copyImageToClipboard,
    downloadFile,
    getMediaUrl,
    isMobile,
    openFile,
    showInExplorer,
  } from "$lib/api/bridge";
  import type { MessageAttachment } from "$lib/state/model";

  interface Props {
    items: MessageAttachment[];
    index: number;
    onclose: () => void;
    onsnackbar?: (msg: string) => void;
  }

  let { items, index = $bindable(0), onclose, onsnackbar }: Props = $props();

  const mobile = isMobile();
  const item = $derived(items[index]);
  let src = $state<string | null>(null);
  let failed = $state(false);
  let scale = $state(1);
  let offsetX = $state(0);
  let offsetY = $state(0);
  let dragging = false;
  let dragStart = { x: 0, y: 0, ox: 0, oy: 0 };
  let touchStartX = 0;
  let root: HTMLDivElement | undefined = $state();

  $effect(() => {
    const current = item;
    if (!current) return;
    let cancelled = false;
    src = null;
    failed = false;
    resetZoom();
    void getMediaUrl(current.path).then((url) => {
      if (cancelled) return;
      src = url;
      failed = !url;
    });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    root?.focus();
  });

  function resetZoom() {
    scale = 1;
    offsetX = 0;
    offsetY = 0;
  }

  function go(step: number) {
    const next = index + step;
    if (next >= 0 && next < items.length) index = next;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onclose();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      go(1);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      go(-1);
    } else if ((event.key === "+" || event.key === "=") && item?.type === "image") {
      scale = Math.min(6, scale * 1.25);
    } else if (event.key === "-" && item?.type === "image") {
      scale = Math.max(1, scale / 1.25);
      if (scale === 1) resetZoom();
    } else if (event.key === "0") {
      resetZoom();
    }
  }

  function handleWheel(event: WheelEvent) {
    if (item?.type !== "image") return;
    event.preventDefault();
    const factor = event.deltaY < 0 ? 1.15 : 1 / 1.15;
    scale = Math.min(6, Math.max(1, scale * factor));
    if (scale === 1) resetZoom();
  }

  function handleDoubleClick() {
    if (item?.type !== "image") return;
    if (scale > 1) resetZoom();
    else scale = 2.5;
  }

  function handlePointerDown(event: PointerEvent) {
    if (scale <= 1 || event.button !== 0) return;
    dragging = true;
    dragStart = { x: event.clientX, y: event.clientY, ox: offsetX, oy: offsetY };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function handlePointerMove(event: PointerEvent) {
    if (!dragging) return;
    offsetX = dragStart.ox + (event.clientX - dragStart.x) / scale;
    offsetY = dragStart.oy + (event.clientY - dragStart.y) / scale;
  }

  function errorText(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  async function copyImage() {
    if (!item) return;
    try {
      await copyImageToClipboard(item.path);
      onsnackbar?.("Image copied");
    } catch (error) {
      onsnackbar?.(`Couldn't copy image: ${errorText(error)}`);
    }
  }

  async function save() {
    if (!item) return;
    try {
      await downloadFile(item.path);
      onsnackbar?.(`Saved ${item.name}`);
    } catch (error) {
      onsnackbar?.(`Save failed: ${errorText(error)}`);
    }
  }
</script>

<div
  bind:this={root}
  class="viewer"
  role="dialog"
  aria-modal="true"
  aria-label={item?.name ?? "Media"}
  tabindex="-1"
  onkeydown={handleKeydown}
  ontouchstart={(event) => (touchStartX = event.touches[0].clientX)}
  ontouchend={(event) => {
    if (scale > 1) return;
    const dx = event.changedTouches[0].clientX - touchStartX;
    if (Math.abs(dx) > 60) go(dx < 0 ? 1 : -1);
  }}
>
  <header class="bar">
    <button class="tool" onclick={onclose} aria-label="Close" title="Close (Esc)"
      ><Icon name="arrow_back" size={22} /></button
    >
    <div class="title">
      <span class="name">{item?.name}</span>
      {#if items.length > 1}<span class="count">{index + 1} of {items.length}</span>{/if}
    </div>
    {#if item?.type === "image" && !mobile}
      <button class="tool" onclick={copyImage} aria-label="Copy image" title="Copy image"
        ><Icon name="content_copy" size={20} /></button
      >
    {/if}
    {#if mobile}
      <button class="tool" onclick={save} aria-label="Save" title="Save"
        ><Icon name="download" size={20} /></button
      >
    {:else}
      <button
        class="tool"
        onclick={() => item && showInExplorer(item.path).catch((e) => onsnackbar?.(errorText(e)))}
        aria-label="Show in folder"
        title="Show in folder"><Icon name="folder_open" size={20} /></button
      >
      <button
        class="tool"
        onclick={() => item && openFile(item.path).catch((e) => onsnackbar?.(errorText(e)))}
        aria-label="Open with default app"
        title="Open with default app"><Icon name="open_in_new" size={20} /></button
      >
    {/if}
  </header>

  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="stage"
    onclick={(event) => {
      if (event.target === event.currentTarget && scale === 1) onclose();
    }}
    onwheel={handleWheel}
  >
    {#if failed}
      <div class="missing">
        <Icon name="hide_image" size={40} />
        <span>This file is no longer available. It may have been moved or deleted.</span>
      </div>
    {:else if src && item?.type === "video"}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video {src} controls autoplay playsinline class="media" onerror={() => (failed = true)}
      ></video>
    {:else if src}
      <img
        {src}
        alt={item?.name}
        class="media"
        class:zoomed={scale > 1}
        draggable="false"
        style:transform="scale({scale}) translate({offsetX}px, {offsetY}px)"
        ondblclick={handleDoubleClick}
        onpointerdown={handlePointerDown}
        onpointermove={handlePointerMove}
        onpointerup={() => (dragging = false)}
        onerror={() => (failed = true)}
      />
    {:else}
      <div class="spinner" aria-label="Loading"></div>
    {/if}

    {#if index > 0}
      <button class="nav nav-prev" onclick={() => go(-1)} aria-label="Previous"
        ><Icon name="chevron_left" size={28} /></button
      >
    {/if}
    {#if index < items.length - 1}
      <button class="nav nav-next" onclick={() => go(1)} aria-label="Next"
        ><Icon name="chevron_right" size={28} /></button
      >
    {/if}
  </div>
</div>

<style>
  .viewer {
    position: fixed;
    inset: 0;
    z-index: 900;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, black 92%, var(--md-sys-color-surface));
    color: white;
    outline: none;
    animation: fade 160ms ease-out;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px;
    flex-shrink: 0;
  }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 0 8px;
  }
  .name {
    font-size: 14px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    font-size: 12px;
    opacity: 0.7;
  }
  .tool {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: none;
    border-radius: 20px;
    background: transparent;
    color: inherit;
    cursor: pointer;
  }
  .tool:hover {
    background: rgb(255 255 255 / 0.12);
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    padding: 0 8px 16px;
  }
  .media {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    border-radius: 4px;
    transition: transform 160ms ease-out;
    user-select: none;
  }
  .media.zoomed {
    cursor: grab;
    transition: none;
  }
  .nav {
    position: absolute;
    top: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    margin-top: -24px;
    border: none;
    border-radius: 24px;
    background: rgb(255 255 255 / 0.12);
    color: white;
    cursor: pointer;
    backdrop-filter: blur(8px);
  }
  .nav:hover {
    background: rgb(255 255 255 / 0.22);
  }
  .nav-prev {
    left: 12px;
  }
  .nav-next {
    right: 12px;
  }
  .missing {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    max-width: 320px;
    text-align: center;
    opacity: 0.8;
    font-size: 14px;
  }
  .spinner {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    border: 4px solid rgb(255 255 255 / 0.2);
    border-top-color: white;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
