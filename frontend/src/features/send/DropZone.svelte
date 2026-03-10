<!--
  File drop zone with drag-over animation.
  Supports: drag & drop multiple files, click to browse.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { pickFiles } from "$lib/api/bridge";

  interface Props {
    onadd?: (paths: string[]) => void;
  }

  let { onadd }: Props = $props();

  const app = getAppState();
  let dragOver = $state(false);

  async function handleClick() {
    const result = await pickFiles();
    if (result && result.length > 0) onadd?.(result);
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    dragOver = true;
  }

  function handleDragLeave(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    // Do NOT call e.stopPropagation() — pywebview's native document-level
    // drop handler needs the event to bubble up to get full file paths
    // (pywebviewFullPath). Paths arrive via "files_dropped" event.
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="drop-zone border-2 border-dashed rounded-xl text-center cursor-pointer
         {app.hasFiles ? 'p-5' : 'p-8'}"
  class:drop-active={dragOver}
  class:drop-selected={!dragOver && app.hasFiles}
  class:drop-idle={!dragOver && !app.hasFiles}
  style="transition: all var(--md-spring-default-spatial-dur) var(--md-spring-default-spatial);"
  onclick={handleClick}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
  role="button"
  tabindex="0"
>
  <div class="flex flex-col items-center gap-1.5">
    <span
      class="icon-wrap"
      class:text-primary={dragOver || app.hasFiles}
      class:text-on-surface-variant={!dragOver && !app.hasFiles}
      style="transition: all var(--md-spring-default-spatial-dur) var(--md-spring-default-spatial);"
    >
      <Icon name={app.hasFiles ? "check_circle" : "cloud_upload"} size={app.hasFiles ? 32 : 48} />
    </span>

    <span class="text-sm text-on-surface-variant">
      {#if app.hasFiles}
        Drop more files here
      {:else}
        Drop files here or click to browse
      {/if}
    </span>

    {#if !app.hasFiles}
      <span class="text-xs text-outline">Supports multiple files and folders</span>
    {/if}
  </div>
</div>

<style>
  .drop-idle {
    border-color: var(--md-sys-color-outline-variant);
  }
  .drop-idle:hover {
    border-color: var(--md-sys-color-primary);
    background: color-mix(in srgb, var(--md-sys-color-primary) 8%, transparent);
  }
  .drop-active {
    border-color: var(--md-sys-color-primary);
    border-style: solid;
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
    transform: scale(1.01);
  }
  .drop-selected {
    border-color: var(--md-sys-color-primary);
    border-style: solid;
    background: color-mix(in srgb, var(--md-sys-color-primary) 8%, transparent);
  }
  .drop-active .icon-wrap {
    transform: scale(1.05);
  }
</style>
