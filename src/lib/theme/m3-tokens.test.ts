import { describe, expect, it } from "vitest";
import type { M3ColorTokens } from "./m3-color";
import { toKebab, tokensToCssVars } from "./m3-tokens";

describe("toKebab", () => {
  it("splits camelCase on capitals", () => {
    expect(toKebab("surface")).toBe("surface");
    expect(toKebab("onSurface")).toBe("on-surface");
    expect(toKebab("onSurfaceVariant")).toBe("on-surface-variant");
    expect(toKebab("surfaceContainerHighest")).toBe("surface-container-highest");
  });
});

describe("tokensToCssVars", () => {
  it("prefixes every token with the M3 system namespace", () => {
    const tokens = {
      primary: "#6750A4",
      onSurfaceVariant: "#49454F",
    } as unknown as M3ColorTokens;

    expect(tokensToCssVars(tokens)).toEqual({
      "--md-sys-color-primary": "#6750A4",
      "--md-sys-color-on-surface-variant": "#49454F",
    });
  });

  it("returns an empty map for empty input", () => {
    expect(tokensToCssVars({} as M3ColorTokens)).toEqual({});
  });
});
