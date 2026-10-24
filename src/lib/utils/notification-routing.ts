import type { AppState } from "$lib/state/app-state.svelte";

/**
 * Open the conversation a notification refers to. Drafts belong to their own
 * conversation, so switching never discards or retargets anything.
 */
export function openNotificationPeer(app: AppState, peerId: string): "opened" | "missing" {
  if (!app.devices.some((device) => device.id === peerId)) return "missing";
  app.activeView = "transfer";
  app.setActiveDevice(peerId);
  return "opened";
}
