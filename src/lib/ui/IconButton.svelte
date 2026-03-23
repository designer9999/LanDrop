<!--
  M3 Icon Button — 4 types: standard, filled, filledTonal, outlined
  Standard: NO visible container, just state layer on 40dp area
  Filled/Tonal: 40dp container with color
  Container touch target: 48dp
-->
<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    variant?: "standard" | "filled" | "filledTonal" | "outlined";
    disabled?: boolean;
    onclick?: () => void;
    title?: string;
    "aria-label"?: string;
    children: Snippet;
  }

  let {
    variant = "standard",
    disabled = false,
    onclick,
    title,
    "aria-label": ariaLabel,
    children,
  }: Props = $props();
</script>

<button
  class="icon-btn"
  class:icon-btn-standard={variant === "standard"}
  class:icon-btn-filled={variant === "filled"}
  class:icon-btn-filledTonal={variant === "filledTonal"}
  class:icon-btn-outlined={variant === "outlined"}
  class:icon-btn-disabled={disabled}
  {disabled}
  {onclick}
  {title}
  aria-label={ariaLabel ?? title}
>
  <span class="icon-btn-state"></span>
  <span class="icon-btn-icon" class:opacity-38={disabled}>
    {@render children()}
  </span>
</button>

<style>
  .icon-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    border: none;
    cursor: pointer;
    user-select: none;
    background: transparent;
    padding: 0;
    margin: 0;
    -webkit-tap-highlight-color: transparent;
  }
  .icon-btn:disabled {
    cursor: not-allowed;
    pointer-events: none;
  }

  /* State layer */
  .icon-btn-state {
    position: absolute;
    inset: 0;
    border-radius: inherit;
    opacity: 0;
    transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .icon-btn:hover .icon-btn-state { opacity: 0.08; }
  .icon-btn:focus-visible .icon-btn-state { opacity: 0.1; }
  .icon-btn:active .icon-btn-state { opacity: 0.1; }

  /* Icon */
  .icon-btn-icon {
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
  }

  /* === Standard: no container, just state layer === */
  .icon-btn-standard {
    color: var(--md-sys-color-on-surface-variant);
  }
  .icon-btn-standard .icon-btn-state {
    background: var(--md-sys-color-on-surface-variant);
  }

  /* === Filled === */
  .icon-btn-filled {
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    transition: box-shadow var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .icon-btn-filled:hover { box-shadow: var(--shadow-level1); }
  .icon-btn-filled .icon-btn-state { background: var(--md-sys-color-on-primary); }

  /* === Filled Tonal === */
  .icon-btn-filledTonal {
    background: var(--md-sys-color-secondary-container);
    color: var(--md-sys-color-on-secondary-container);
    transition: box-shadow var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .icon-btn-filledTonal:hover { box-shadow: var(--shadow-level1); }
  .icon-btn-filledTonal .icon-btn-state { background: var(--md-sys-color-on-secondary-container); }

  /* === Outlined === */
  .icon-btn-outlined {
    color: var(--md-sys-color-on-surface-variant);
    border: 1px solid var(--md-sys-color-outline-variant);
  }
  .icon-btn-outlined .icon-btn-state { background: var(--md-sys-color-on-surface-variant); }

  /* === Disabled states === */
  .icon-btn-disabled.icon-btn-filled,
  .icon-btn-disabled.icon-btn-filledTonal {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent) !important;
    box-shadow: none !important;
  }
  .icon-btn-disabled.icon-btn-outlined {
    border-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent) !important;
  }
</style>
