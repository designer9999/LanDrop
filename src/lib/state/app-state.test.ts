import { describe, expect, it } from "vitest";
import { AppState } from "./app-state.svelte";
import type { DiscoveredPeer } from "$lib/api/bridge";

const laptop: DiscoveredPeer = {
  id: "laptop",
  alias: "Laptop",
  device_type: "desktop",
  ip: "192.168.1.10",
  port: 53318,
  network: "lan",
  lan_ip: "192.168.1.10",
};

describe("session device discovery", () => {
  it("preserves files and text queued while an earlier file batch is sending", () => {
    const app = new AppState();
    app.addFile("/outgoing/first.txt", null);
    app.addFile("/outgoing/second.txt", null);
    const outgoingBatch = [...app.files];
    app.transferActive = true;
    app.addFile("/outgoing/next.txt", null);
    app.removeFile("/outgoing/first.txt");
    app.addFile("/outgoing/first.txt", null);
    app.sendTextContent = "Send this next";
    app.removeSentFiles(outgoingBatch);
    expect(app.filePaths).toEqual(["/outgoing/next.txt", "/outgoing/first.txt"]);
    expect(app.sendTextContent).toBe("Send this next");
  });

  it("restores history and preferences without selecting or showing a stale device online", () => {
    const previous = new AppState();
    previous.upsertDevice(laptop);
    previous.updateDeviceSettings(laptop.id, { color: 3, avatarIcon: "computer" });
    previous.addMessage({ peerId: laptop.id, direction: "sent", text: "Saved conversation" });
    const saved = { ...previous.exportPersistedState(), activeDeviceId: laptop.id };
    const restored = new AppState();
    restored.hydratePersistedState(saved);
    expect(restored.onlineDevices).toEqual([]);
    expect(restored.activeDevice).toBeNull();
    expect(restored.devices[0]).toMatchObject({ color: 3, avatarIcon: "computer", ip: "" });
    expect(restored.getPeerMessages(laptop.id)).toHaveLength(1);
    restored.upsertDevice(laptop);
    expect(restored.onlineDevices).toHaveLength(1);
    expect(restored.activeDevice?.color).toBe(3);
    expect(restored.getPeerMessages(laptop.id)[0].text).toBe("Saved conversation");
  });

  it("merges LAN and Tailscale updates into the same conversation", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    app.addMessage({ peerId: laptop.id, direction: "received", text: "hello" });
    app.upsertDevice({
      ...laptop,
      network: "tailscale",
      ip: "100.64.0.1",
      tailscale_ip: "100.64.0.1",
    });
    expect(app.onlineDevices).toHaveLength(1);
    expect(app.activeDevice).toMatchObject({
      network: "tailscale",
      lanIp: laptop.ip,
      tailscaleIp: "100.64.0.1",
    });
    expect(app.getPeerMessages(laptop.id)).toHaveLength(1);
  });

  it("clears a departing recipient without redirecting their draft to another device", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    app.upsertDevice({ ...laptop, id: "other" });
    app.sendTextContent = "Only for Laptop";
    app.markDeviceOffline(laptop.id);
    app.upsertDevice({ ...laptop, id: "third" });
    expect(app.activeDevice).toBeNull();
    expect(app.activeDeviceOnline).toBe(false);
    expect(app.sendTextContent).toBe("Only for Laptop");
    expect(app.onlineDevices.map((device) => device.id)).toEqual(["other", "third"]);
    expect(app.devices).toHaveLength(3);
  });

  it("does not select offline history and exits all-history mode for an available device", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    app.markDeviceOffline(laptop.id);
    app.setActiveDevice(laptop.id);
    expect(app.activeDevice).toBeNull();
    app.upsertDevice(laptop);
    app.messageViewAll = true;
    app.messageSearch = "old";
    app.setActiveDevice(laptop.id);
    expect(app.messageViewAll).toBe(false);
    expect(app.messageSearch).toBe("");
  });
});
