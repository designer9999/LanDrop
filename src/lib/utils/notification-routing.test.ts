import { describe, expect, it } from "vitest";
import { AppState } from "$lib/state/app-state.svelte";
import { openNotificationPeer } from "./notification-routing";

function setup() {
  const app = new AppState();
  for (const id of ["sender", "other"]) {
    app.upsertDevice({ id, alias: id, ip: "192.168.1.2", device_type: "desktop" });
  }
  app.setActiveDevice("other");
  return app;
}

describe("notification conversation routing", () => {
  it("opens the exact peer and clears history filters", () => {
    const app = setup();
    app.activeView = "settings";
    app.starredOnly = true;
    app.messageSearch = "old search";
    expect(openNotificationPeer(app, "sender")).toBe("opened");
    expect(app.activeDeviceId).toBe("sender");
    expect(app.activeView).toBe("transfer");
    expect(app.starredOnly).toBe(false);
    expect(app.messageSearch).toBe("");
  });
  it("opens an offline saved peer without making it discoverable or sendable", () => {
    const app = setup();
    app.markDeviceOffline("sender");
    expect(openNotificationPeer(app, "sender")).toBe("opened");
    expect(app.activeDeviceOnline).toBe(false);
    expect(app.onlineDevices.map((device) => device.id)).toEqual(["other"]);
  });
  it("rejects unknown targets", () => {
    const app = setup();
    expect(openNotificationPeer(app, "unknown")).toBe("missing");
    expect(app.activeDeviceId).toBe("other");
  });
  it("keeps each conversation's draft with its own peer", () => {
    const app = setup();
    app.sendTextContent = "Private draft";
    app.addFile("/draft.txt", null);
    expect(openNotificationPeer(app, "sender")).toBe("opened");
    expect(app.activeDeviceId).toBe("sender");
    expect(app.sendTextContent).toBe("");
    expect(app.filePaths).toEqual([]);
    expect(openNotificationPeer(app, "other")).toBe("opened");
    expect(app.sendTextContent).toBe("Private draft");
    expect(app.filePaths).toEqual(["/draft.txt"]);
  });
});
