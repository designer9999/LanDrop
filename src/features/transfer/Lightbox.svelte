<!--
  Fullscreen image lightbox with header actions.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { showInExplorer, openFile, isMobile } from "$lib/api/bridge";

  interface Props {
    src: string;
    name: string;
    path: string;
    loading: boolean;
    onclose: () => void;
  }

  let { src, name, path, loading, onclose }: Props = $props();
  const mobile = isMobile();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="lightbox" onclick={onclose}>
  <div class="lightbox-header">
    <span class="lightbox-name">{name}</span>
    <div class="lightbox-actions">
      {#if mobile}
        <button class="lightbox-btn" onclick={(e) => { e.stopPropagation(); openFile(path); }} title="Open / Share">
          <Icon name="open_in_new" size={18} />
        </button>
      {:else}
        <button class="lightbox-btn" onclick={(e) => { e.stopPropagation(); showInExplorer(path); }} title="Open in folder">
          <Icon name="folder_open" size={18} />
        </button>
      {/if}
      <button class="lightbox-close" onclick={onclose}>
        <Icon name="close" size={20} />
      </button>
    </div>
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <img {src} alt={name} class="lightbox-img" class:lightbox-img-loading={loading} onclick={(e) => e.stopPropagation()} />
  {#if loading}
    <span class="lightbox-loading">Loading full image...</span>
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
    from { opacity: 0; }
    to { opacity: 1; }
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
    background: linear-gradient(rgba(0,0,0,0.6), transparent);
  }
  .lightbox-name {
    font-size: 12px;
    color: rgba(255,255,255,0.8);
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
  .lightbox-btn, .lightbox-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: none;
    border-radius: 50%;
    background: rgba(255,255,255,0.15);
    color: #fff;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .lightbox-btn:hover, .lightbox-close:hover {
    background: rgba(255,255,255,0.25);
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
    color: rgba(255,255,255,0.5);
  }
</style>
