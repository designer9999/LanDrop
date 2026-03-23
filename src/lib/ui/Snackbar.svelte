<!--
  Compact inline notification — text bar at bottom, not a floating popup.
  Auto-dismiss 3s. Slides in from below.
-->
<script lang="ts">
  interface Props {
    message?: string;
    visible?: boolean;
    ondismiss?: () => void;
  }

  let { message = "", visible = $bindable(false), ondismiss }: Props = $props();

  $effect(() => {
    if (visible) {
      const timer = setTimeout(() => { visible = false; ondismiss?.(); }, 4000);
      return () => clearTimeout(timer);
    }
  });
</script>

{#if visible}
  <div class="toast-bar" role="status" aria-live="polite">
    <span class="toast-text">{message}</span>
  </div>
{/if}

<style>
  .toast-bar {
    position: fixed;
    bottom: 40px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    border-radius: 99px;
    background: var(--md-sys-color-inverse-surface);
    color: var(--md-sys-color-inverse-on-surface);
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    max-width: 90%;
    box-shadow: 0 2px 8px rgba(0,0,0,0.25);
    animation: toast-in 0.2s ease both;
    pointer-events: none;
  }
  .toast-text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @keyframes toast-in {
    from { opacity: 0; transform: translateX(-50%) translateY(8px); }
    to   { opacity: 1; transform: translateX(-50%) translateY(0); }
  }
</style>
