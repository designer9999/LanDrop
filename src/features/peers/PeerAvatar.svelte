<!--
  Peer avatar — colored circle with optional icon or first letter
  Sizes: sm (24px), md (32px), lg (40px)
-->
<script lang="ts">
  import { PEER_COLORS } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  interface Props {
    name: string;
    color: number;
    icon?: string;
    size?: "sm" | "md" | "lg";
    selected?: boolean;
  }

  let { name, color, icon, size = "md", selected = false }: Props = $props();

  const initial = $derived(name.charAt(0).toUpperCase());
  const bg = $derived(PEER_COLORS[color % PEER_COLORS.length]);
  const px = $derived(size === "sm" ? 24 : size === "md" ? 32 : 40);
  const fs = $derived(size === "sm" ? 11 : size === "md" ? 14 : 18);
  const iconSize = $derived(size === "sm" ? 15 : size === "md" ? 19 : 24);
</script>

<div
  class="shrink-0 rounded-full flex items-center justify-center font-medium text-white select-none"
  style="width: {px}px; height: {px}px; font-size: {fs}px; background-color: {bg};{selected ? ` box-shadow: inset 0 0 0 2px rgba(255,255,255,0.7);` : ''}"
>
  {#if icon}
    <Icon name={icon} size={iconSize} />
  {:else}
    {initial}
  {/if}
</div>
