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
    app.activeSends = 1;
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

  it("keeps a departing peer's conversation open, read-only, with its draft", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    app.upsertDevice({ ...laptop, id: "other" });
    app.sendTextContent = "Only for Laptop";
    app.markDeviceOffline(laptop.id);
    app.upsertDevice({ ...laptop, id: "third" });
    expect(app.activeDevice?.id).toBe(laptop.id);
    expect(app.activeDeviceOnline).toBe(false);
    expect(app.sendTextContent).toBe("Only for Laptop");
    app.setActiveDevice("other");
    expect(app.sendTextContent).toBe("");
    expect(app.onlineDevices.map((device) => device.id)).toEqual(["other", "third"]);
    expect(app.devices).toHaveLength(3);
  });

  it("opens offline history and clears filters when switching conversations", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    app.upsertDevice({ ...laptop, id: "other" });
    app.markDeviceOffline(laptop.id);
    app.setActiveDevice(laptop.id);
    expect(app.activeDevice?.id).toBe(laptop.id);
    expect(app.activeDeviceOnline).toBe(false);
    app.starredOnly = true;
    app.messageSearch = "old";
    app.setActiveDevice("other");
    expect(app.starredOnly).toBe(false);
    expect(app.messageSearch).toBe("");
  });

  it("moves attachments queued before any conversation into the first one opened", () => {
    const app = new AppState();
    app.addFile("/hotkey/pick.png", null);
    app.upsertDevice(laptop);
    expect(app.activeDeviceId).toBe(laptop.id);
    expect(app.filePaths).toEqual(["/hotkey/pick.png"]);
  });

  it("counts unread messages until the conversation is opened", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    app.upsertDevice({ ...laptop, id: "other" });
    app.receiveText({ peerId: "other", text: "ping", id: "m1" });
    expect(app.conversations.find((c) => c.device.id === "other")?.unread).toBe(1);
    app.setActiveDevice("other");
    expect(app.conversations.find((c) => c.device.id === "other")?.unread).toBe(0);
  });

  it("restores the pre-edit draft when an edit is cancelled", () => {
    const app = new AppState();
    app.upsertDevice(laptop);
    const sent = app.addMessage({ peerId: laptop.id, direction: "sent", text: "typo" });
    app.sendTextContent = "half-written";
    app.startEditing(sent.id);
    expect(app.sendTextContent).toBe("typo");
    app.stopEditing();
    expect(app.sendTextContent).toBe("half-written");
    expect(app.lastEditableMessage(laptop.id)?.id).toBe(sent.id);
  });
});
