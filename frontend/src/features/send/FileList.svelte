<!--
  File list — shows selected files as compact thumbnails (images) or cards (other files).
  Claude-style: images as small square thumbnails in a row, files as compact chips.
  Uses base64 data URIs via backend API (WebView2 blocks file:/// cross-origin).
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { getThumbnail } from "$lib/api/bridge";

  const app = getAppState();

  const IMAGE_EXTS = new Set([".jpg", ".jpeg", ".png", ".gif", ".svg", ".webp", ".bmp", ".ico"]);

  function isImage(type: string): boolean {
    return IMAGE_EXTS.has(type);
  }

  function iconForType(type: string): string {
    if (type === "folder") return "folder";
    if (isImage(type)) return "image";
    if ([".mp4", ".avi", ".mkv", ".mov", ".webm"].includes(type)) return "movie";
    if ([".mp3", ".wav", ".flac", ".ogg", ".aac"].includes(type)) return "audio_file";
    if ([".zip", ".rar", ".7z", ".tar", ".gz"].includes(type)) return "folder_zip";
    if ([".pdf"].includes(type)) return "picture_as_pdf";
    if ([".doc", ".docx", ".txt", ".rtf", ".md"].includes(type)) return "description";
    if ([".xls", ".xlsx", ".csv"].includes(type)) return "table_chart";
    return "draft";
  }

  // Cache for base64 thumbnails keyed by file path
  let thumbCache = $state<Record<string, string>>({});

  // Load thumbnails for image files
  $effect(() => {
    for (const file of images) {
      if (!thumbCache[file.path]) {
        getThumbnail(file.path).then(uri => {
          if (uri) thumbCache = { ...thumbCache, [file.path]: uri };
        });
      }
    }
  });

  const images = $derived(app.files.filter(f => f.info && isImage(f.info.type)));
  const otherFiles = $derived(app.files.filter(f => !f.info || !isImage(f.info.type)));
</script>

<!-- Image thumbnails row (Claude-style compact squares) -->
{#if images.length > 0}
  <div class="thumb-row">
    {#each images as file (file.path)}
      <div class="thumb-wrap group">
        {#if thumbCache[file.path]}
          <img
            src={thumbCache[file.path]}
            alt={file.info?.name ?? "image"}
            class="thumb-img"
          />
        {:else}
          <div class="thumb-placeholder">
            <Icon name="image" size={20} />
          </div>
        {/if}
        {#if !app.transferActive}
          <button
            class="thumb-remove"
            onclick={() => app.removeFile(file.path)}
            title="Remove"
          >
            <Icon name="close" size={12} />
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<!-- Non-image files as compact chips -->
{#if otherFiles.length > 0}
  <div class="file-chips">
    {#each otherFiles as file (file.path)}
      <div class="file-chip group">
        <span class="text-primary shrink-0">
          <Icon name={file.info ? iconForType(file.info.type) : "draft"} size={16} />
        </span>
        <div class="file-chip-info">
          <span class="file-chip-name">{file.info?.name ?? file.path.split(/[\\/]/).pop()}</span>
          {#if file.info}
            <span class="file-chip-size">{file.info.size}</span>
          {/if}
        </div>
        {#if !app.transferActive}
          <button
            class="file-chip-remove"
            onclick={() => app.removeFile(file.path)}
            title="Remove"
          >
            <Icon name="close" size={14} />
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  /* ── Image Thumbnails ── */
  .thumb-row {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .thumb-wrap {
    position: relative;
    width: 56px;
    height: 56px;
    border-radius: 8px;
    overflow: hidden;
    background: var(--md-sys-color-surface-container);
    flex-shrink: 0;
  }
  .thumb-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .thumb-placeholder {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.4;
  }
  .thumb-remove {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease;
    padding: 0;
  }
  .thumb-wrap:hover .thumb-remove {
    opacity: 1;
  }

  /* ── File Chips ── */
  .file-chips {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .file-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 5%, transparent);
    transition: background 0.15s ease;
  }
  .file-chip:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .file-chip-info {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .file-chip-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--md-sys-color-on-surface);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .file-chip-size {
    font-size: 10px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.6;
    flex-shrink: 0;
  }
  .file-chip-remove {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: none;
    background: transparent;
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease;
    padding: 0;
    flex-shrink: 0;
  }
  .file-chip:hover .file-chip-remove {
    opacity: 1;
  }
  .file-chip-remove:hover {
    color: var(--md-sys-color-error);
  }
</style>
