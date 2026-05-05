<!--
  M3 Dialog — confirmation/input dialog
  Shape: 28dp corners, 280-560dp width, surfaceContainerHigh bg
  Motion: fade + scale spring, focus trap, Escape to dismiss
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";

  interface Props {
    open: boolean;
    headline?: string;
    id?: string;
    onclose?: () => void;
    onconfirm?: () => void;
    confirmLabel?: string;
    dismissLabel?: string;
    confirmDisabled?: boolean;
    children: Snippet;
  }

  let {
    open = $bindable(false),
    headline,
    id = `dialog-${Math.random().toString(36).slice(2, 8)}`,
    onclose,
    onconfirm,
    confirmLabel = "Confirm",
    dismissLabel = "Cancel",
    confirmDisabled = false,
    children,
  }: Props = $props();

  let headlineId = $derived(`${id}-headline`);
  let dialogEl: HTMLDivElement | undefined = $state();
  let exiting = $state(false);
  let exitTimer: ReturnType<typeof setTimeout> | undefined;

  function handleScrimClick() { startExit(); }
  function handleDismiss() { startExit(); }
  function handleConfirm() { onconfirm?.(); }

  function startExit() {
    if (exiting) return;
    exiting = true;
    clearTimeout(exitTimer);
    exitTimer = setTimeout(finishExit, 400);
  }

  function finishExit() {
    clearTimeout(exitTimer);
    if (!exiting) return;
    exiting = false;
    open = false;
    onclose?.();
  }

  function handleExitEnd(e: AnimationEvent) {
    if (exiting && e.animationName.includes("scale-out")) {
      finishExit();
    }
  }

  $effect(() => () => clearTimeout(exitTimer));

  const FOCUSABLE_SELECTOR =
    'input:not([disabled]), textarea:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      handleDismiss();
    } else if (e.key === "Tab") {
      if (!dialogEl) return;
      const focusables = Array.from(dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
      if (focusables.length === 0) return;
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    } else if (e.key === "Enter" && !confirmDisabled) {
      if ((e.target as HTMLElement)?.tagName === "TEXTAREA") return;
      e.preventDefault();
      handleConfirm();
    }
  }

  $effect(() => {
    if (open && !exiting) {
      tick().then(() => {
        if (!dialogEl) return;
        const focusable = dialogEl.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
        focusable?.focus();
      });
    }
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-50 flex items-center justify-center"
    onkeydown={handleKeydown}
  >
    <div
      class="absolute inset-0 bg-scrim/32"
      class:dialog-scrim-enter={!exiting}
      class:dialog-scrim-exit={exiting}
      onclick={handleScrimClick}
      onkeydown={() => {}}
      role="presentation"
    ></div>

    <div
      bind:this={dialogEl}
      class="relative z-10 bg-surface-container-high text-on-surface
             min-w-70 max-w-140 w-full mx-6
             shadow-level3"
      class:dialog-enter={!exiting}
      class:dialog-exit={exiting}
      onanimationend={handleExitEnd}
      style="border-radius: 28px; --tf-bg: var(--color-surface-container-high);"
      role="dialog"
      aria-modal="true"
      aria-labelledby={headline ? headlineId : undefined}
    >
      <div class="p-6 space-y-4 dialog-content-area">
        {#if headline}
          <h2 id={headlineId} class="text-2xl font-normal">
            {headline}
          </h2>
        {/if}
        <div class="text-sm text-on-surface-variant">
          {@render children()}
        </div>
      </div>
      <div class="flex justify-end gap-2 px-6 pb-6">
        <Button variant="outlined" onclick={handleDismiss}>{dismissLabel}</Button>
        <Button disabled={confirmDisabled} onclick={handleConfirm}>{confirmLabel}</Button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-scrim-enter {
    animation: scrim-fade-in var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) forwards;
  }
  .dialog-scrim-exit {
    animation: scrim-fade-out var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) forwards;
  }
  .dialog-enter {
    animation:
      dialog-scale-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) forwards,
      dialog-fade-in var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) forwards;
  }
  .dialog-exit {
    animation:
      dialog-scale-out var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) forwards,
      dialog-fade-out var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) forwards;
  }
  @keyframes scrim-fade-in { from { opacity: 0; } to { opacity: 1; } }
  @keyframes scrim-fade-out { from { opacity: 1; } to { opacity: 0; } }
  @keyframes dialog-scale-in { from { transform: scale(0.85); } to { transform: scale(1); } }
  @keyframes dialog-fade-in { from { opacity: 0; } to { opacity: 1; } }
  @keyframes dialog-scale-out { from { transform: scale(1); } to { transform: scale(0.85); } }
  @keyframes dialog-fade-out { from { opacity: 1; } to { opacity: 0; } }

  .dialog-content-area {
    max-height: 60vh;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-on-surface) 20%, transparent) transparent;
  }
  .dialog-content-area::-webkit-scrollbar { width: 6px; }
  .dialog-content-area::-webkit-scrollbar-track { background: transparent; }
  .dialog-content-area::-webkit-scrollbar-thumb {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 20%, transparent);
    border-radius: 3px;
  }
</style>
