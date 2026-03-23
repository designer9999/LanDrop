<!--
  Main transfer view — welcome onboarding + chat layout
-->
<script lang="ts">
  import { getAppState } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ChatArea from "./ChatArea.svelte";

  interface Props {
    onsnackbar?: (msg: string) => void;
    onaddpeer?: () => void;
    onsend?: () => void;
    onsendtext?: () => void;
  }

  let { onsnackbar, onaddpeer, onsend, onsendtext }: Props = $props();

  const app = getAppState();
  const peer = $derived(app.activePeer);
</script>

{#if !app.peers.length}
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
          Transfer files instantly between devices on your local network. No internet required.
        </p>
      </div>

      <div class="welcome-steps">
        <div class="step">
          <span class="step-num">1</span>
          <span class="step-text">Add a peer with a shared code phrase</span>
        </div>
        <div class="step">
          <span class="step-num">2</span>
          <span class="step-text">Both devices auto-connect on the same network</span>
        </div>
        <div class="step">
          <span class="step-num">3</span>
          <span class="step-text">Drop files or send messages — instant transfer</span>
        </div>
      </div>

      <Button onclick={onaddpeer}>
        <Icon name="person_add" size={18} />
        Add your first peer
      </Button>
    </div>
  </div>
{/if}

<ChatArea peerName={peer?.name} {onsnackbar} {onsend} {onsendtext} />

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
</style>
