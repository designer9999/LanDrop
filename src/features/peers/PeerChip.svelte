<!--
  Peer chip — avatar + name, selected state, LAN indicator
-->
<script lang="ts">
  import type { Peer } from "$lib/state/app-state.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";
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
  class="group relative inline-flex items-center gap-1.5 h-8 pl-1 pr-3 rounded-sm
         text-xs font-medium cursor-pointer select-none overflow-hidden shrink-0"
  style="
    background-color: {selected ? 'var(--md-sys-color-secondary-container)' : 'transparent'};
    border: {selected ? '1px solid transparent' : '1px solid var(--md-sys-color-outline-variant)'};
    color: {selected ? 'var(--md-sys-color-on-secondary-container)' : 'var(--md-sys-color-on-surface-variant)'};
    transition:
      background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  "
  {onclick}
>
  <div
    class="absolute inset-0 opacity-0
           group-hover:opacity-8 group-focus-visible:opacity-10 group-active:opacity-10
           {selected ? 'bg-on-secondary-container' : 'bg-on-surface-variant'}"
    style="transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
  ></div>

  <span class="relative z-10">
    <PeerAvatar name={peer.name} color={peer.color} size="sm" />
  </span>
  <span class="relative z-10 max-w-24 truncate">{peer.name}</span>
  {#if selected && app.lanConnected}
    <span class="relative z-10 w-1.5 h-1.5 rounded-full bg-primary animate-pulse"></span>
  {:else if selected}
    <span class="relative z-10 text-on-secondary-container opacity-50"
          style="font-size: 14px; line-height: 1;">
      <Icon name="edit" size={14} />
    </span>
  {/if}
</button>
