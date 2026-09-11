import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type NotificationTarget = { kind: "peer"; peerId: string } | { kind: "updates" };
export interface NotificationActivation {
  id: string;
  target: NotificationTarget;
}

let permissionResolved = false;
let permissionGranted = false;

const INCOMING_NOTIFICATION_ID = 1001;
const INCOMING_NOTIFICATION_GROUP = "landrop";

async function ensurePermission(): Promise<boolean> {
  if (permissionResolved) return permissionGranted;

  permissionGranted = await isPermissionGranted();
  if (!permissionGranted) {
    permissionGranted = (await requestPermission()) === "granted";
  }

  permissionResolved = true;
  return permissionGranted;
}

export async function sendNativeNotification(
  title: string,
  body: string,
  target: NotificationTarget,
): Promise<void> {
  // Android receive notifications are emitted from Rust so they still work
  // when the WebView is backgrounded or paused.
  if (/Android/i.test(navigator.userAgent)) return;
  if (/Windows/i.test(navigator.userAgent)) {
    await invoke("show_native_notification", {
      notification: { title, body, target, silent: true },
    });
    return;
  }
  if (!(await ensurePermission())) return;
  await sendNotification({
    id: INCOMING_NOTIFICATION_ID,
    group: INCOMING_NOTIFICATION_GROUP,
    title,
    body,
  });
}

/** Subscribe before draining cold-start activations; event/queue races are deduplicated. */
export async function onNotificationActivation(
  callback: (target: NotificationTarget) => void,
): Promise<() => void> {
  if (!/Windows/i.test(navigator.userAgent)) return () => {};
  const seen = new Set<string>();
  let disposed = false;
  function deliver(activation: NotificationActivation) {
    if (disposed || !activation?.id || seen.has(activation.id)) return;
    const target = activation.target;
    if (
      !target ||
      (target.kind !== "updates" && !(target.kind === "peer" && typeof target.peerId === "string"))
    )
      return;
    seen.add(activation.id);
    if (seen.size > 128) seen.delete(seen.values().next().value!);
    callback(target);
  }
  let drainQueue = Promise.resolve();
  function drain() {
    const current = drainQueue.then(async () => {
      if (disposed) return;
      const pending = await invoke<NotificationActivation[]>("take_notification_activations");
      pending.forEach(deliver);
    });
    // Keep later wakeups usable after a failed IPC call.
    drainQueue = current.catch(() => {});
    return current;
  }
  const unlisten = await listen<NotificationActivation>("native-notification-activated", () => {
    // Events only wake the queue reader. Delivering them immediately could let an
    // older startup-drain response override the user's newest click.
    void drain().catch(() => {});
  });
  try {
    await drain();
  } catch (error) {
    disposed = true;
    unlisten();
    throw error;
  }
  return () => {
    disposed = true;
    unlisten();
  };
}
