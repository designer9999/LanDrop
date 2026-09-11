import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  send: vi.fn(),
  granted: vi.fn(),
  permission: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  sendNotification: mocks.send,
  isPermissionGranted: mocks.granted,
  requestPermission: mocks.permission,
}));

beforeEach(() => {
  vi.resetModules();
  vi.resetAllMocks();
  vi.stubGlobal("navigator", { userAgent: "Windows NT 10.0" });
  mocks.invoke.mockResolvedValue([]);
  mocks.listen.mockResolvedValue(mocks.unlisten);
});
afterEach(() => vi.unstubAllGlobals());

describe("native notifications", () => {
  it("delivers an older startup action before a newer live click even when IPC is delayed", async () => {
    let wake!: (event: unknown) => void;
    let resolveStartup!: (value: unknown) => void;
    const older = { id: "old", target: { kind: "peer", peerId: "old-peer" } };
    const newer = { id: "new", target: { kind: "peer", peerId: "new-peer" } };
    mocks.listen.mockImplementation(async (_event, callback) => {
      wake = callback;
      return mocks.unlisten;
    });
    mocks.invoke.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveStartup = resolve;
      }),
    );
    mocks.invoke.mockResolvedValueOnce([newer]);
    const { onNotificationActivation } = await import("./native-notifications");
    const callback = vi.fn();
    const starting = onNotificationActivation(callback);
    await vi.waitFor(() => expect(mocks.invoke).toHaveBeenCalledOnce());
    wake({ payload: newer });
    expect(callback).not.toHaveBeenCalled();
    expect(mocks.invoke).toHaveBeenCalledOnce();
    resolveStartup([older]);
    const stop = await starting;
    await vi.waitFor(() => expect(callback).toHaveBeenCalledTimes(2));
    expect(callback.mock.calls.map(([target]) => target.peerId)).toEqual(["old-peer", "new-peer"]);
    stop();
  });

  it("routes Windows to the native backend with a navigation-only target", async () => {
    const { sendNativeNotification } = await import("./native-notifications");
    await sendNativeNotification("Sender", "New file", { kind: "peer", peerId: "peer" });
    expect(mocks.invoke).toHaveBeenCalledWith("show_native_notification", {
      notification: {
        title: "Sender",
        body: "New file",
        target: { kind: "peer", peerId: "peer" },
        silent: true,
      },
    });
    expect(mocks.send).not.toHaveBeenCalled();
  });
  it("does not duplicate Android's backend notification", async () => {
    vi.stubGlobal("navigator", { userAgent: "Android" });
    const { sendNativeNotification } = await import("./native-notifications");
    await sendNativeNotification("Sender", "Message", { kind: "updates" });
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(mocks.send).not.toHaveBeenCalled();
  });
  it("respects denied notification permission on other desktops", async () => {
    vi.stubGlobal("navigator", { userAgent: "Macintosh" });
    mocks.granted.mockResolvedValue(false);
    mocks.permission.mockResolvedValue("denied");
    const { sendNativeNotification } = await import("./native-notifications");
    await sendNativeNotification("Sender", "Message", { kind: "updates" });
    expect(mocks.send).not.toHaveBeenCalled();
  });
  it("deduplicates a live action also present in the startup queue and cleans up", async () => {
    const activation = { id: "click-1", target: { kind: "updates" } };
    mocks.listen.mockImplementation(async (_event, callback) => {
      callback({ payload: activation });
      return mocks.unlisten;
    });
    mocks.invoke.mockResolvedValue([activation]);
    const { onNotificationActivation } = await import("./native-notifications");
    const callback = vi.fn();
    const stop = await onNotificationActivation(callback);
    expect(callback).toHaveBeenCalledExactlyOnceWith({ kind: "updates" });
    stop();
    expect(mocks.unlisten).toHaveBeenCalledOnce();
  });
  it("removes the listener if startup draining fails", async () => {
    mocks.invoke.mockRejectedValue(new Error("not available"));
    const { onNotificationActivation } = await import("./native-notifications");
    await expect(onNotificationActivation(vi.fn())).rejects.toThrow("not available");
    expect(mocks.unlisten).toHaveBeenCalledOnce();
  });
});
