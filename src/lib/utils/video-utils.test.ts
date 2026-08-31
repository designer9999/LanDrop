import { describe, expect, it } from "vitest";
import { formatDuration, seekRatio } from "./video-utils";

describe("formatDuration", () => {
  it("pads seconds and rolls over minutes", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(9)).toBe("0:09");
    expect(formatDuration(59.9)).toBe("0:59");
    expect(formatDuration(60)).toBe("1:00");
    expect(formatDuration(605)).toBe("10:05");
  });

  it("renders a placeholder for unknown durations", () => {
    expect(formatDuration(NaN)).toBe("0:00");
    expect(formatDuration(Infinity)).toBe("0:00");
  });
});

describe("seekRatio", () => {
  it("clamps to the 0..1 range", () => {
    const rect = { left: 100, width: 200 };
    expect(seekRatio(100, rect)).toBe(0);
    expect(seekRatio(200, rect)).toBe(0.5);
    expect(seekRatio(300, rect)).toBe(1);
    expect(seekRatio(0, rect)).toBe(0);
    expect(seekRatio(9999, rect)).toBe(1);
  });

  it("is safe on a zero-width bar", () => {
    expect(seekRatio(50, { left: 0, width: 0 })).toBe(0);
  });
});
