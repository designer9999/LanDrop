<!--
  Peer avatar — colored circle with first letter
  Sizes: sm (24px), md (32px), lg (40px)
-->
<script lang="ts">
  import { PEER_COLORS } from "$lib/state/app-state.svelte";

  interface Props {
    name: string;
    color: number;
    size?: "sm" | "md" | "lg";
  }

  let { name, color, size = "md" }: Props = $props();

  const initial = $derived(name.charAt(0).toUpperCase());
  const bg = $derived(PEER_COLORS[color % PEER_COLORS.length]);
  const px = $derived(size === "sm" ? 24 : size === "md" ? 32 : 40);
  const fs = $derived(size === "sm" ? 11 : size === "md" ? 14 : 18);
</script>

<div
  class="shrink-0 rounded-full flex items-center justify-center font-medium text-white select-none"
  style="width: {px}px; height: {px}px; font-size: {fs}px; background-color: {bg};"
>
  {initial}
</div>
