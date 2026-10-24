import { describe, expect, it } from "vitest";
import type { MessageEntry } from "$lib/state/model";
import { buildTimeline, dateLabel } from "./timeline";

function msg(
  id: string,
  timestamp: string,
  direction: "sent" | "received" = "received",
): MessageEntry {
  return { id, peerId: "p", direction, text: id, timestamp };
}

const now = new Date(2026, 9, 24, 12, 0, 0);
const at = (day: number, hour: number, minute = 0) =>
  new Date(2026, 9, day, hour, minute).toISOString();

describe("buildTimeline", () => {
  it("inserts a date separator per day and groups close messages", () => {
    const items = buildTimeline(
      [
        msg("a", at(23, 9)),
        msg("b", at(24, 9)),
        msg("c", at(24, 9, 2)),
        msg("d", at(24, 9, 3), "sent"),
      ],
      { now },
    );
    expect(
      items.map((item) =>
        item.kind === "message"
          ? `${item.message.id}:${item.position}`
          : item.kind === "date"
            ? item.label
            : item.kind,
      ),
    ).toEqual(["Yesterday", "a:solo", "Today", "b:first", "c:last", "d:solo"]);
  });

  it("splits groups separated by more than five minutes", () => {
    const items = buildTimeline([msg("a", at(24, 9)), msg("b", at(24, 9, 10))], { now });
    const positions = items.flatMap((item) => (item.kind === "message" ? [item.position] : []));
    expect(positions).toEqual(["solo", "solo"]);
  });

  it("places one unread divider before the first new received message", () => {
    const items = buildTimeline(
      [
        msg("a", at(24, 9)),
        msg("b", at(24, 10), "sent"),
        msg("c", at(24, 11)),
        msg("d", at(24, 11, 1)),
      ],
      { now, unreadAfter: at(24, 10) },
    );
    const kinds = items.map((item) => item.kind);
    expect(kinds.filter((kind) => kind === "unread")).toHaveLength(1);
    const unread = items.findIndex((item) => item.kind === "unread");
    expect(items[unread + 1]).toMatchObject({ message: { id: "c" } });
    expect(items[unread]).toMatchObject({ count: 2 });
  });
});

describe("dateLabel", () => {
  it("names recent days and formats older ones", () => {
    expect(dateLabel(new Date(2026, 9, 24, 8), now)).toBe("Today");
    expect(dateLabel(new Date(2026, 9, 23, 8), now)).toBe("Yesterday");
    expect(dateLabel(new Date(2025, 0, 2), now)).toMatch(/2025/);
  });
});
