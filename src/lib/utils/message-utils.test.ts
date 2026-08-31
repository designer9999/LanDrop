import { describe, expect, it } from "vitest";
import { getBubblePosition, type BubbleNeighbor } from "./message-utils";

const sent = (peerId = "p1"): BubbleNeighbor => ({ direction: "sent", peerId });
const received = (peerId = "p1"): BubbleNeighbor => ({ direction: "received", peerId });

describe("getBubblePosition", () => {
  it("marks a lone message as solo", () => {
    expect(getBubblePosition([sent()], 0)).toBe("solo");
  });

  it("groups a same-direction, same-peer run", () => {
    const msgs = [sent(), sent(), sent()];
    expect(getBubblePosition(msgs, 0)).toBe("first");
    expect(getBubblePosition(msgs, 1)).toBe("middle");
    expect(getBubblePosition(msgs, 2)).toBe("last");
  });

  it("breaks a group when the direction changes", () => {
    const msgs = [sent(), received(), sent()];
    expect(getBubblePosition(msgs, 0)).toBe("solo");
    expect(getBubblePosition(msgs, 1)).toBe("solo");
    expect(getBubblePosition(msgs, 2)).toBe("solo");
  });

  it("breaks a group when the peer changes (all-peers view)", () => {
    const msgs = [sent("p1"), sent("p2"), sent("p2")];
    expect(getBubblePosition(msgs, 0)).toBe("solo");
    expect(getBubblePosition(msgs, 1)).toBe("first");
    expect(getBubblePosition(msgs, 2)).toBe("last");
  });
});
