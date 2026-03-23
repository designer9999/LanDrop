<!--
  M3 Switch
  - Track: 52x32dp, rounded-full
  - Handle: 16dp (off) -> 24dp (on) -> 28dp (pressed)
  - Selected: primary track, onPrimary handle
  - Unselected: surfaceContainerHighest track, 2dp outline border, outline handle
  - State layer: 40dp circle centered on handle
  - Motion: fast spatial for handle, fast effects for color
-->
<script lang="ts">
  interface Props {
    checked?: boolean;
    disabled?: boolean;
    "aria-label"?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    disabled = false,
    "aria-label": ariaLabel,
    onchange,
  }: Props = $props();

  let pressed = $state(false);
  let hovered = $state(false);

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }

  let handleSize = $derived(pressed ? 28 : checked ? 24 : 16);
  let handleLeft = $derived(
    checked
      ? (pressed ? 20 : 24)
      : (pressed ? 2 : 6)
  );
</script>

<button
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel}
  class="relative inline-flex items-center justify-center w-13 h-12
         cursor-pointer select-none
         disabled:cursor-not-allowed"
  {disabled}
  onclick={toggle}
  onpointerdown={() => { if (!disabled) pressed = true; }}
  onpointerup={() => (pressed = false)}
  onpointerleave={() => { pressed = false; hovered = false; }}
  onpointerenter={() => { if (!disabled) hovered = true; }}
>
  <span
    class="relative inline-flex items-center w-13 h-8 rounded-full"
    class:bg-primary={checked && !disabled}
    class:bg-surface-container-highest={!checked && !disabled}
    class:border-2={!checked}
    class:border-outline={!checked && !disabled}
    class:switch-disabled-track-selected={disabled && checked}
    class:switch-disabled-track-unselected={disabled && !checked}
    style="transition: background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
                       border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
  >
    <span
      class="absolute rounded-full"
      style="
        width: {handleSize}px;
        height: {handleSize}px;
        left: {handleLeft}px;
        top: 50%;
        transform: translateY(-50%);
        transition: width var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
                    height var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
                    left var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
                    background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
      "
      class:bg-on-primary={checked && !disabled}
      class:bg-outline={!checked && !disabled}
      class:switch-disabled-handle-selected={disabled && checked}
      class:switch-disabled-handle-unselected={disabled && !checked}
    >
      {#if !disabled}
        <span
          class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-10 h-10
                 rounded-full opacity-0"
          class:bg-primary={checked}
          class:bg-on-surface={!checked}
          class:opacity-8={hovered && !pressed}
          class:opacity-10={pressed}
          style="transition: opacity var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
        ></span>
      {/if}
    </span>
  </span>
</button>

<style>
  .switch-disabled-track-selected {
    background-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent) !important;
  }
  .switch-disabled-track-unselected {
    background-color: color-mix(in srgb, var(--md-sys-color-surface-container-highest) 12%, transparent) !important;
    border-color: color-mix(in srgb, var(--md-sys-color-on-surface) 12%, transparent) !important;
  }
  .switch-disabled-handle-selected {
    background-color: var(--md-sys-color-surface) !important;
  }
  .switch-disabled-handle-unselected {
    background-color: color-mix(in srgb, var(--md-sys-color-on-surface) 38%, transparent) !important;
  }
</style>
