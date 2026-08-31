/**
 * Notification sounds using Web Audio API.
 * No external files needed — generates clean tones programmatically.
 */

let audioCtx: AudioContext | null = null;

function getCtx(): AudioContext {
  if (!audioCtx) audioCtx = new AudioContext();
  return audioCtx;
}

/** Short two-tone chime — file/message received */
export function playReceiveSound() {
  const ctx = getCtx();
  const now = ctx.currentTime;

  // Two ascending notes: E5 → G5
  playTone(ctx, 659.25, now, 0.12, 0.15);
  playTone(ctx, 783.99, now + 0.12, 0.15, 0.18);
}

function playTone(
  ctx: AudioContext,
  freq: number,
  start: number,
  duration: number,
  volume: number,
) {
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();

  osc.type = "sine";
  osc.frequency.value = freq;
  osc.connect(gain);
  gain.connect(ctx.destination);

  gain.gain.setValueAtTime(0, start);
  gain.gain.linearRampToValueAtTime(volume, start + 0.01);
  gain.gain.exponentialRampToValueAtTime(0.001, start + duration);

  osc.start(start);
  osc.stop(start + duration + 0.01);
}
