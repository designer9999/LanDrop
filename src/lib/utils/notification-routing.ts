import type { AppState } from "$lib/state/app-state.svelte";

/** Explicit notification navigation never invents a discoverable device or retargets a draft. */
export function openNotificationPeer(
  app: AppState,
  peerId: string,
): "opened" | "missing" | "busy" | "draft" {
  if (!app.devices.some((device) => device.id === peerId)) return "missing";
  if (app.transferActive) return "busy";
  if (peerId !== app.activeDeviceId && (app.sendTextContent || app.hasFiles)) return "draft";
  app.activeView = "transfer";
  app.activeDeviceId = peerId;
  app.messageViewAll = false;
  app.messageSearch = "";
  return "opened";
}
