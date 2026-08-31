/** Pure helpers shared by the inline video tile and the lightbox player. */

/** Format seconds as `m:ss`; non-finite/zero input renders as `0:00`. */
export function formatDuration(seconds: number): string {
  if (!seconds || !isFinite(seconds)) return "0:00";
  const minutes = Math.floor(seconds / 60);
  const rest = Math.floor(seconds % 60);
  return `${minutes}:${rest.toString().padStart(2, "0")}`;
}

/** Clamped 0..1 position of `clientX` within `rect` — for click-to-seek bars. */
export function seekRatio(clientX: number, rect: { left: number; width: number }): number {
  if (rect.width <= 0) return 0;
  return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
}
