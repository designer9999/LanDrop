<!--
  Drag-and-drop overlay with M3 expressive animations.
  Rendered when files are dragged over the chat area.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
</script>

<div class="drag-overlay">
  <div class="drop-border"></div>
  <div class="drop-content">
    <div class="drop-icon-ring">
      <div class="drop-icon">
        <Icon name="upload_file" size={32} />
      </div>
    </div>
    <span class="drop-title">Drop files here</span>
    <span class="drop-subtitle">Files will be sent to your peer</span>
  </div>
</div>

<style>
  .drag-overlay {
    position: absolute;
    inset: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--md-sys-color-surface) 92%, transparent);
    backdrop-filter: blur(8px);
    pointer-events: none;
    /* M3: expressive default effects — 200ms */
    animation: overlay-fade 200ms cubic-bezier(0.34, 0.8, 0.34, 1) both;
  }
  @keyframes overlay-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .drop-border {
    position: absolute;
    inset: 16px;
    border: 2px dashed var(--md-sys-color-primary);
    border-radius: 24px;
    opacity: 0.5;
    /* M3: expressive default spatial — 500ms */
    animation: border-draw 500ms cubic-bezier(0.38, 1.21, 0.22, 1) both;
  }
  @keyframes border-draw {
    from {
      inset: 40px;
      opacity: 0;
      border-radius: 40px;
    }
    to {
      inset: 16px;
      opacity: 0.5;
      border-radius: 24px;
    }
  }
  .drop-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    /* M3: expressive fast spatial — 350ms, staggered 100ms */
    animation: content-up 350ms cubic-bezier(0.42, 1.67, 0.21, 0.9) 100ms both;
  }
  @keyframes content-up {
    from {
      opacity: 0;
      transform: translateY(16px) scale(0.9);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  .drop-icon-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 72px;
    height: 72px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--md-sys-color-primary) 12%, transparent);
    /* M3: standard easing for continuous ambient loop */
    animation: ring-pulse 1.5s cubic-bezier(0.2, 0, 0, 1) infinite;
  }
  @keyframes ring-pulse {
    0%,
    100% {
      transform: scale(1);
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent);
    }
    50% {
      transform: scale(1.05);
      box-shadow: 0 0 0 12px color-mix(in srgb, var(--md-sys-color-primary) 0%, transparent);
    }
  }
  .drop-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    /* M3: expressive fast spatial — 350ms, staggered 200ms for icon entrance */
    animation: icon-enter 350ms cubic-bezier(0.42, 1.67, 0.21, 0.9) 200ms both;
  }
  @keyframes icon-enter {
    from {
      transform: scale(0);
    }
    to {
      transform: scale(1);
    }
  }
  .drop-title {
    font-size: 16px;
    font-weight: 500;
    color: var(--md-sys-color-on-surface);
    letter-spacing: 0.15px;
  }
  .drop-subtitle {
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }
</style>
