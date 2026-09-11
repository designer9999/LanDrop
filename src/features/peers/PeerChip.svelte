<!--
  Peer chip — avatar with online/offline status dot
-->
<script lang="ts">
  import type { DiscoveredDevice } from "$lib/state/app-state.svelte";
  import PeerAvatar from "./PeerAvatar.svelte";
  import { peerRouteDescription, peerRouteLabel } from "$lib/utils/peer-utils";

  interface Props {
    device: DiscoveredDevice;
    selected: boolean;
    onclick: () => void;
  }

  let { device, selected, onclick }: Props = $props();
</script>

<button
  class="peer-chip"
  class:peer-selected={selected}
  {onclick}
  title="{device.alias} · {peerRouteDescription(device)}{selected ? ' · Open device settings' : ''}"
  aria-label="{device.alias}, {peerRouteDescription(device)}{selected
    ? ', open device settings'
    : ''}"
  aria-pressed={selected}
>
  <PeerAvatar
    name={device.alias}
    color={device.color}
    icon={device.avatarIcon}
    size="sm"
    {selected}
  />
  {#if device.online}
    <span class="status-dot" class:selected></span>
  {:else}
    <span class="status-dot offline"></span>
  {/if}
  <span class="peer-label">
    <span class="peer-name">{device.alias}</span>
    <span class="peer-route">{peerRouteLabel(device)}</span>
  </span>
</button>

<style>
  .peer-chip {
    position: relative;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 48px;
    padding: 8px;
    gap: 8px;
    border-radius: 8px;
    border: 1px solid var(--md-sys-color-outline);
    background: transparent;
    cursor: pointer;
    transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .peer-chip:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  .peer-selected {
    background: var(--md-sys-color-secondary-container);
    border-color: transparent;
  }
  .peer-selected .peer-label,
  .peer-selected .peer-route {
    color: var(--md-sys-color-on-secondary-container);
  }
  .peer-selected:hover {
    background: color-mix(
      in srgb,
      var(--md-sys-color-on-secondary-container) 8%,
      var(--md-sys-color-secondary-container)
    );
  }
  .peer-label {
    display: flex;
    flex-direction: column;
    text-align: left;
    color: var(--md-sys-color-on-surface);
    max-width: 100px;
  }
  .peer-name {
    font-size: 14px;
    font-weight: 500;
    line-height: 20px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .peer-route {
    font-size: 11px;
    line-height: 16px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .peer-chip:active {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent);
  }
  .status-dot {
    position: absolute;
    bottom: 10px;
    left: 26px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--md-sys-color-tertiary);
    border: 2px solid var(--md-sys-color-surface);
  }
  .status-dot.selected {
    background: var(--md-sys-color-primary);
    animation: pulse-glow 2s cubic-bezier(0.2, 0, 0, 1) infinite;
  }
  .status-dot.offline {
    background: var(--md-sys-color-outline);
  }
  @keyframes pulse-glow {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }
</style>
