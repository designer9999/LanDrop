<!--
  Peer chip — avatar only with availability dot
-->
<script lang="ts">
  import type { Peer } from "$lib/state/app-state.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import PeerAvatar from "./PeerAvatar.svelte";

  interface Props {
    peer: Peer;
    selected: boolean;
    onclick: () => void;
  }

  let { peer, selected, onclick }: Props = $props();
  const app = getAppState();
</script>

<button
  class="peer-chip"
  {onclick}
  title={peer.name}
>
  <PeerAvatar name={peer.name} color={peer.color} size="sm" />
  {#if selected && app.lanConnected}
    <span class="status-dot"></span>
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
    background: var(--md-sys-color-primary);
    border: 2px solid var(--md-sys-color-surface);
    animation: pulse-glow 2s cubic-bezier(0.2, 0.0, 0, 1.0) infinite;
  }
  @keyframes pulse-glow {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
</style>
