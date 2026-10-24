import { describe, expect, it } from "vitest";
import { isEmojiOnly, tokenizeMessage } from "./rich-text";

describe("tokenizeMessage", () => {
  it("links URLs and trims trailing punctuation", () => {
    expect(tokenizeMessage("see https://example.com/a, ok")).toEqual([
      { type: "text", value: "see " },
      { type: "link", value: "https://example.com/a", href: "https://example.com/a" },
      { type: "text", value: ", ok" },
    ]);
  });

  it("keeps balanced parentheses inside links", () => {
    const [, link] = tokenizeMessage("x https://en.wikipedia.org/wiki/Foo_(bar)");
    expect(link).toMatchObject({ href: "https://en.wikipedia.org/wiki/Foo_(bar)" });
  });

  it("upgrades bare www links to https", () => {
    expect(tokenizeMessage("www.example.org")[0]).toEqual({
      type: "link",
      value: "www.example.org",
      href: "https://www.example.org",
    });
  });

  it("extracts inline code and fenced blocks without linking inside them", () => {
    expect(tokenizeMessage("run `npm i` then\n```ts\nconst u = 'https://x.y';\n```\ndone")).toEqual(
      [
        { type: "text", value: "run " },
        { type: "code", value: "npm i" },
        { type: "text", value: " then" },
        { type: "codeblock", lang: "ts", value: "const u = 'https://x.y';" },
        { type: "text", value: "done" },
      ],
    );
  });

  it("never treats javascript: or other schemes as links", () => {
    expect(tokenizeMessage("javascript:alert(1)")).toEqual([
      { type: "text", value: "javascript:alert(1)" },
    ]);
  });
});

describe("isEmojiOnly", () => {
  it("detects short emoji-only messages", () => {
    expect(isEmojiOnly("👍")).toBe(true);
    expect(isEmojiOnly("❤️ 🔥")).toBe(true);
    expect(isEmojiOnly("👨‍👩‍👧")).toBe(true);
  });

  it("rejects text, digits and long runs", () => {
    expect(isEmojiOnly("ok 👍")).toBe(false);
    expect(isEmojiOnly("123")).toBe(false);
    expect(isEmojiOnly("👍👍👍👍")).toBe(false);
  });
});
