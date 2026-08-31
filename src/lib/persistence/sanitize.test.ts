import { describe, expect, it } from "vitest";
import {
  devicesForPersistence,
  normalizeHydratedDevices,
  sanitizeAttachment,
  sanitizeAttachments,
  sanitizeMessage,
  sanitizeMessages,
} from "./sanitize";
import type { DiscoveredDevice, MessageEntry } from "$lib/state/model";

describe("sanitizeAttachment", () => {
  it("keeps a well-formed attachment and drops unknown fields", () => {
    const result = sanitizeAttachment({
      name: "a.png",
      path: "/dl/a.png",
      size: "1.0 KB",
      type: "image",
      bogus: 1,
    });
    expect(result).toEqual({ name: "a.png", path: "/dl/a.png", size: "1.0 KB", type: "image" });
  });

  it("derives a missing name from the path", () => {
    expect(sanitizeAttachment({ path: "C:\\dl\\photo.png" })?.name).toBe("photo.png");
  });

  it("falls back to the file type for an unknown type", () => {
    expect(sanitizeAttachment({ name: "a", path: "/a", type: "wat" })?.type).toBe("file");
  });

  it("promotes an attachment with children to a folder and counts them", () => {
    const result = sanitizeAttachment({
      name: "docs",
      path: "/dl/docs",
      type: "file",
      children: [
        { name: "a.txt", path: "/dl/docs/a.txt", type: "file" },
        { name: "b.txt", path: "/dl/docs/b.txt", type: "file" },
      ],
    });
    expect(result?.type).toBe("folder");
    expect(result?.children).toHaveLength(2);
    expect(result?.fileCount).toBe(2);
  });

  it("caps nesting at one level", () => {
    const result = sanitizeAttachment({
      name: "top",
      path: "/dl/top",
      children: [
        {
          name: "sub",
          path: "/dl/top/sub",
          children: [{ name: "deep.txt", path: "/dl/top/sub/deep.txt" }],
        },
      ],
    });
    expect(result?.children?.[0].children).toBeUndefined();
  });

  it("rejects non-records and fully empty entries", () => {
    expect(sanitizeAttachment(null)).toBeNull();
    expect(sanitizeAttachment("nope")).toBeNull();
    expect(sanitizeAttachment({ name: "", path: "" })).toBeNull();
  });
});

describe("sanitizeAttachments", () => {
  it("wraps a single record and drops invalid entries", () => {
    expect(sanitizeAttachments({ name: "a", path: "/a" })).toHaveLength(1);
    expect(sanitizeAttachments([{ name: "a", path: "/a" }, null, 3])).toHaveLength(1);
  });

  it("returns undefined rather than an empty array", () => {
    expect(sanitizeAttachments([])).toBeUndefined();
    expect(sanitizeAttachments("nope")).toBeUndefined();
  });
});

describe("sanitizeMessage", () => {
  it("round-trips a valid message unchanged", () => {
    const message = {
      id: "m1",
      peerId: "p1",
      direction: "sent",
      text: "hello",
      timestamp: "2026-01-01T00:00:00.000Z",
      starred: true,
    };
    expect(sanitizeMessage(message)).toEqual(message);
  });

  it("requires a peer id and a known direction", () => {
    expect(sanitizeMessage({ direction: "sent" })).toBeNull();
    expect(sanitizeMessage({ peerId: "p1", direction: "sideways" })).toBeNull();
    expect(sanitizeMessage(null)).toBeNull();
  });

  it("fills in a generated id and timestamp", () => {
    const result = sanitizeMessage({ peerId: "p1", direction: "received" });
    expect(result?.id).toBeTruthy();
    expect(Number.isNaN(Date.parse(result?.timestamp ?? ""))).toBe(false);
    expect(result?.text).toBe("");
  });

  it("only keeps starred when it is exactly true", () => {
    expect(
      sanitizeMessage({ peerId: "p1", direction: "sent", starred: "yes" })?.starred,
    ).toBeUndefined();
  });
});

describe("sanitizeMessages", () => {
  it("filters invalid entries and returns an array for non-arrays", () => {
    expect(sanitizeMessages([{ peerId: "p1", direction: "sent" }, {}, null])).toHaveLength(1);
    expect(sanitizeMessages("nope")).toEqual([]);
  });
});

describe("normalizeHydratedDevices", () => {
  it("normalizes stored devices and forces them offline", () => {
    const devices = normalizeHydratedDevices(
      [
        {
          id: "p1",
          alias: "Laptop",
          deviceType: "desktop",
          ip: "192.168.1.5",
          color: 3,
          online: true,
        },
      ],
      [],
    );
    expect(devices).toHaveLength(1);
    expect(devices[0].online).toBe(false);
    expect(devices[0].alias).toBe("Laptop");
    expect(devices[0].color).toBe(3);
  });

  it("never restores a persisted receive folder (Rust owns it)", () => {
    const devices = normalizeHydratedDevices([{ id: "p1", outFolder: "C:\\stale" }], []);
    expect(devices[0].outFolder).toBeUndefined();
  });

  it("accepts the snake_case device_type from older snapshots", () => {
    const devices = normalizeHydratedDevices([{ id: "p1", device_type: "mobile" }], []);
    expect(devices[0].deviceType).toBe("mobile");
  });

  it("drops entries without an id and non-records", () => {
    expect(normalizeHydratedDevices([{ alias: "no id" }, null, 5], [])).toEqual([]);
    expect(normalizeHydratedDevices("nope", [])).toEqual([]);
  });

  it("creates placeholders for peers that only exist in history", () => {
    const messages = [
      { id: "m1", peerId: "ghostpeer-1234", direction: "received", text: "", timestamp: "t" },
    ] as MessageEntry[];
    const devices = normalizeHydratedDevices([], messages);
    expect(devices).toHaveLength(1);
    expect(devices[0].id).toBe("ghostpeer-1234");
    expect(devices[0].alias).toBe("Device-ghostpee");
    expect(devices[0].online).toBe(false);
  });

  it("does not duplicate a peer that is already known", () => {
    const messages = [
      { id: "m1", peerId: "p1", direction: "sent", text: "", timestamp: "t" },
    ] as MessageEntry[];
    expect(normalizeHydratedDevices([{ id: "p1" }], messages)).toHaveLength(1);
  });
});

describe("devicesForPersistence", () => {
  it("clears runtime-only and backend-owned fields", () => {
    const devices = [
      {
        id: "p1",
        alias: "A",
        deviceType: "desktop",
        ip: "1.2.3.4",
        online: true,
        color: 0,
        outFolder: "C:\\x",
      },
    ] as DiscoveredDevice[];
    const persisted = devicesForPersistence(devices);
    expect(persisted[0].online).toBe(false);
    expect(persisted[0].outFolder).toBeUndefined();
    expect(persisted[0].alias).toBe("A");
  });
});
