<!--
  Shared file/folder card — used by chat bubbles and the composer tray.

  Owns the card chrome (name, size, badge, hover state); callers supply an
  optional leading icon, extra body content (e.g. a folder preview strip) and
  a corner action button. Give a hover-revealed action the `file-card-action`
  class to have it fade in with the card.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "$lib/ui/Icon.svelte";

  interface Props {
    name: string;
    size?: string;
    badge: string;
    icon?: string;
    /** Narrower 120px variant used in the composer tray. */
    compact?: boolean;
    title?: string;
    onclick?: (e: MouseEvent) => void;
    action?: Snippet;
    children?: Snippet;
  }

  let {
    name,
    size,
    badge,
    icon,
    compact = false,
    title,
    onclick,
    action,
    children,
  }: Props = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="file-card" class:file-card-compact={compact} {onclick} {title}>
  {#if icon}
    <Icon name={icon} size={18} />
  {/if}
  <span class="file-card-name">{name}</span>
  {#if size}
    <span class="file-card-size">{size}</span>
  {/if}
  <span class="file-card-badge">{badge}</span>
  {@render children?.()}
  {@render action?.()}
</div>

<style>
  .file-card {
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
  .file-card-compact {
    width: 120px;
    flex-shrink: 0;
  }
  .file-card:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 10%, transparent);
  }
  .file-card-name {
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
  .file-card-size {
    font-size: 11px;
    opacity: 0.5;
  }
  .file-card-badge {
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

  /* Action buttons come from the caller's scope — reveal on card hover. */
  .file-card:hover :global(.file-card-action) {
    opacity: 1;
  }
</style>
