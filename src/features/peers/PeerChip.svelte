<!--
  Peer chip — avatar with online/offline status dot
-->
<script lang="ts">
  import type { DiscoveredDevice } from "$lib/state/app-state.svelte";
  import PeerAvatar from "./PeerAvatar.svelte";

  interface Props {
    device: DiscoveredDevice;
    selected: boolean;
    onclick: () => void;
  }

  let { device, selected, onclick }: Props = $props();
</script>

<button
  class="peer-chip"
  {onclick}
  title="{device.alias}{device.online ? '' : ' (offline)'}"
>
  <PeerAvatar name={device.alias} color={device.color} size="sm" />
  {#if device.online}
    <span class="status-dot" class:selected></span>
  {:else}
    <span class="status-dot offline"></span>
  {/if}
</button>

<style>
  .peer-chip {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    border-radius: 50%;
    border: none;
    background: transparent;
    cursor: pointer;
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .peer-chip:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .peer-chip:active {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
  }
  .status-dot {
    position: absolute;
    bottom: 2px;
    right: 2px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--md-sys-color-tertiary);
    border: 2px solid var(--md-sys-color-surface);
  }
  .status-dot.selected {
    background: var(--md-sys-color-primary);
    animation: pulse-glow 2s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
  }
  .status-dot.offline {
    background: var(--md-sys-color-outline);
  }
  @keyframes pulse-glow {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
</style>
