<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: "elevated" | "filled" | "outlined";
    children: Snippet;
  }

  let { variant = "filled", children }: Props = $props();
</script>

<div
  class="p-4 rounded-md text-on-surface
         {variant === 'elevated'
           ? 'bg-surface-container-low shadow-level1'
           : variant === 'filled'
             ? 'bg-surface-container-highest'
             : 'bg-surface border border-outline-variant'}"
>
  {@render children()}
</div>
