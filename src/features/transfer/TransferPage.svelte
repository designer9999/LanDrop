<!--
  Main transfer view — welcome onboarding + chat layout
-->
<script lang="ts">
  import { getAppState } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import ChatArea from "./ChatArea.svelte";

  interface Props {
    onsnackbar?: (msg: string) => void;
    onsend?: () => void;
    onsendtext?: () => void;
  }

  let { onsnackbar, onsend, onsendtext }: Props = $props();

  const app = getAppState();
  const device = $derived(app.activeDevice);
</script>

{#if !app.devices.length}
  <div class="welcome-wrapper">
    <div class="welcome-card">
      <div class="welcome-icon-ring">
        <div class="welcome-icon">
          <Icon name="swap_horiz" size={32} />
        </div>
      </div>

      <div class="welcome-text">
        <h2 class="welcome-title">Welcome to LanDrop</h2>
        <p class="welcome-desc">
          Transfer files instantly between devices on your local network. No setup required.
        </p>
      </div>

      <div class="welcome-steps">
        <div class="step">
          <span class="step-num">1</span>
          <span class="step-text">Install LanDrop on your devices</span>
        </div>
        <div class="step">
          <span class="step-num">2</span>
          <span class="step-text">Devices on the same network appear automatically</span>
        </div>
        <div class="step">
          <span class="step-num">3</span>
          <span class="step-text">Drop files or send messages — instant transfer</span>
        </div>
      </div>

      <div class="searching-hint">
        <Icon name="radar" size={18} />
        <span>Searching for devices on your network...</span>
      </div>
    </div>
  </div>
{/if}

<ChatArea peerName={device?.alias} {onsnackbar} {onsend} {onsendtext} />

<style>
  .welcome-wrapper {
    padding: 24px 16px 8px;
  }

  .welcome-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 20px;
    padding: 32px 24px;
    border-radius: 28px;
    background: var(--md-sys-color-surface-container);
    animation: welcome-in var(--md-spring-default-spatial-dur) var(--md-spring-default-spatial) both;
  }

  @keyframes welcome-in {
    from { opacity: 0; transform: translateY(12px) scale(0.97); }
    to   { opacity: 1; transform: translateY(0) scale(1); }
  }

  .welcome-icon-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 72px;
    height: 72px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
  }
  .welcome-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
  }

  .welcome-text {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .welcome-title {
    font-size: 22px;
    font-weight: 400;
    line-height: 28px;
    color: var(--md-sys-color-on-surface);
    margin: 0;
  }
  .welcome-desc {
    font-size: 14px;
    line-height: 20px;
    color: var(--md-sys-color-on-surface-variant);
    margin: 0;
    max-width: 320px;
  }

  .welcome-steps {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    max-width: 300px;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
  }
  .step-num {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--md-sys-color-tertiary-container);
    color: var(--md-sys-color-on-tertiary-container);
    font-size: 12px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .step-text {
    font-size: 13px;
    line-height: 18px;
    color: var(--md-sys-color-on-surface-variant);
  }

  .searching-hint {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.7;
    animation: pulse-opacity 2s ease-in-out infinite;
  }

  @keyframes pulse-opacity {
    0%, 100% { opacity: 0.7; }
    50% { opacity: 0.4; }
  }
</style>
