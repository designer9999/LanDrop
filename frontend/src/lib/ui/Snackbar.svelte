<!--
  M3 Snackbar — inverseSurface, auto-dismiss 4s
-->
<script lang="ts">
  interface Props {
    message?: string;
    visible?: boolean;
    ondismiss?: () => void;
  }

  let { message = "", visible = $bindable(false), ondismiss }: Props = $props();

  let exiting = $state(false);

  $effect(() => {
    if (visible) {
      const timer = setTimeout(() => { exiting = true; }, 4000);
      return () => clearTimeout(timer);
    }
  });

  function handleAnimationEnd() {
    if (exiting) {
      exiting = false;
      visible = false;
      ondismiss?.();
    }
  }
</script>

{#if visible}
  <div
    class="fixed bottom-6 left-1/2 -translate-x-1/2 z-50
           flex items-center gap-2 px-6 py-3.5 rounded-sm
           bg-inverse-surface text-inverse-on-surface
           shadow-level3 text-sm w-fit max-w-[90%]"
    class:snackbar-enter={!exiting}
    class:snackbar-exit={exiting}
    onanimationend={handleAnimationEnd}
    role="status"
    aria-live="polite"
  >
    {message}
  </div>
{/if}

<style>
  .snackbar-enter {
    animation:
      snackbar-slide-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both,
      snackbar-fade-in var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) both;
  }
  .snackbar-exit {
    animation:
      snackbar-slide-out var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) forwards,
      snackbar-fade-out var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) forwards;
  }
  @keyframes snackbar-slide-in {
    from { transform: translateX(-50%) translateY(100%); }
    to   { transform: translateX(-50%) translateY(0); }
  }
  @keyframes snackbar-fade-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
  @keyframes snackbar-slide-out {
    from { transform: translateX(-50%) translateY(0); }
    to   { transform: translateX(-50%) translateY(100%); }
  }
  @keyframes snackbar-fade-out {
    from { opacity: 1; }
    to   { opacity: 0; }
  }
</style>
