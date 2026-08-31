<!--
  M3 Slider — Standard, XS size (16dp track)
  Narrow pill handle (4dp wide, 44dp tall), active/inactive track
-->
<script lang="ts">
  interface Props {
    min?: number;
    max?: number;
    step?: number;
    value?: number;
    "aria-label"?: string;
    oninput?: (value: number) => void;
  }

  let {
    min = 0,
    max = 100,
    step = 1,
    value = $bindable(50),
    "aria-label": ariaLabel,
    oninput,
  }: Props = $props();

  let trackEl: HTMLDivElement | undefined = $state();
  let pressed = $state(false);

  const pct = $derived(((value - min) / (max - min)) * 100);

  function clampValue(raw: number): number {
    const stepped = Math.round(raw / step) * step;
    return Math.max(min, Math.min(max, stepped));
  }

  function getClientX(e: PointerEvent | MouseEvent | TouchEvent): number {
    if ("touches" in e && e.touches.length > 0) return e.touches[0].clientX;
    if ("clientX" in e) return e.clientX;
    return 0;
  }

  function updateFromEvent(e: PointerEvent | MouseEvent | TouchEvent) {
    if (!trackEl) return;
    const rect = trackEl.getBoundingClientRect();
    const clientX = getClientX(e);
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const newVal = clampValue(min + ratio * (max - min));
    if (newVal !== value) {
      value = newVal;
      oninput?.(newVal);
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 && e.pointerType === "mouse") return;
    e.preventDefault();
    pressed = true;
    updateFromEvent(e);
    (e.target as HTMLElement)?.setPointerCapture?.(e.pointerId);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
  }

  function onPointerMove(e: PointerEvent) {
    updateFromEvent(e);
  }

  function onPointerUp() {
    pressed = false;
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
    window.removeEventListener("pointercancel", onPointerUp);
  }

  function onKeydown(e: KeyboardEvent) {
    let newVal: number;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      newVal = clampValue(value + step);
    } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      newVal = clampValue(value - step);
    } else if (e.key === "Home") {
      newVal = min;
    } else if (e.key === "End") {
      newVal = max;
    } else {
      return;
    }
    e.preventDefault();
    if (newVal !== value) {
      value = newVal;
      oninput?.(newVal);
    }
  }
</script>

<div
  class="m3-slider"
  bind:this={trackEl}
  onpointerdown={onPointerDown}
  onkeydown={onKeydown}
  role="slider"
  tabindex="0"
  aria-label={ariaLabel}
  aria-orientation="horizontal"
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={value}
>
  <!-- Active track -->
  <div class="track-active" style="width: {pct}%">
    <!-- Stop indicator at start -->
    <div class="stop stop-active-start"></div>
  </div>

  <!-- Handle -->
  <div class="handle-zone" style="left: {pct}%">
    <div class="handle" class:handle-pressed={pressed}></div>
  </div>

  <!-- Inactive track -->
  <div class="track-inactive" style="width: {100 - pct}%">
    <!-- Stop indicator at end -->
    <div class="stop stop-inactive-end"></div>
  </div>
</div>

<style>
  .m3-slider {
    position: relative;
    display: flex;
    align-items: center;
    height: 44px; /* touch target */
    cursor: pointer;
    outline: none;
    touch-action: none; /* prevent scroll while dragging */
    -webkit-tap-highlight-color: transparent;
  }
  .m3-slider:focus-visible .handle {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--md-sys-color-primary) 40%, transparent);
  }

  /* ── Tracks ── */
  .track-active {
    height: 16px;
    border-radius: 8px;
    background: var(--md-sys-color-primary);
    position: relative;
    min-width: 8px;
    display: flex;
    align-items: center;
  }
  .track-inactive {
    height: 16px;
    border-radius: 8px;
    background: var(--md-sys-color-secondary-container);
    position: relative;
    min-width: 8px;
    display: flex;
    align-items: center;
  }

  /* ── Stop indicators ── */
  .stop {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    position: absolute;
  }
  .stop-active-start {
    left: 6px;
    background: var(--md-sys-color-on-primary);
  }
  .stop-inactive-end {
    right: 6px;
    background: var(--md-sys-color-on-secondary-container);
  }

  /* ── Handle ── */
  .handle-zone {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 6px; /* gap between track and handle */
  }
  .handle {
    width: 4px;
    height: 44px;
    border-radius: 2px;
    background: var(--md-sys-color-primary);
    transition:
      width var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      height var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      border-radius var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .handle-pressed {
    width: 2px;
    height: 40px;
  }
</style>
