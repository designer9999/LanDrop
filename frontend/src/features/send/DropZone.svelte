<!--
  File drop zone with drag-over animation.
  Supports: drag & drop multiple files, click to browse.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";

  interface Props {
    onadd?: (paths: string[]) => void;
  }

  let { onadd }: Props = $props();

  const app = getAppState();
  let dragOver = $state(false);

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragOver = true;
  }

  function handleDragLeave(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragOver = false;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragOver = false;

    const files = e.dataTransfer?.files;
    if (files && files.length > 0) {
      const paths: string[] = [];
      for (let i = 0; i < files.length; i++) {
        const f = files[i] as any;
        const path = f.path || f.name;
        if (path) paths.push(path);
      }
      if (paths.length > 0) onadd?.(paths);
    }
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
