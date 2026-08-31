import { describe, expect, it } from "vitest";
import { VideoLruCache } from "./video-cache";

function makeCache(budget: number) {
  const revoked: string[] = [];
  const cache = new VideoLruCache(budget, (url) => revoked.push(url));
  return { cache, revoked };
}

describe("VideoLruCache", () => {
  it("hands back an inserted url and counts references", () => {
    const { cache } = makeCache(1000);
    cache.insert("a", "blob:a", 10);

    expect(cache.refCount("a")).toBe(1);
    expect(cache.acquire("a")).toBe("blob:a");
    expect(cache.refCount("a")).toBe(2);
    expect(cache.size).toBe(1);
    expect(cache.bytes).toBe(10);
  });

  it("returns null for an unknown path", () => {
    const { cache } = makeCache(1000);
    expect(cache.acquire("missing")).toBeNull();
  });

  it("never evicts an entry that is still referenced", () => {
    const { cache, revoked } = makeCache(100);
    cache.insert("a", "blob:a", 60);
    cache.insert("b", "blob:b", 60);

    // Over budget, but both are held by a live consumer.
    expect(cache.bytes).toBe(120);
    expect(revoked).toEqual([]);
    expect(cache.size).toBe(2);
  });

  it("evicts once the last reference is released and the budget is exceeded", () => {
    const { cache, revoked } = makeCache(100);
    cache.insert("a", "blob:a", 60);
    cache.insert("b", "blob:b", 60);

    expect(cache.release("blob:a")).toBe(true);
    expect(revoked).toEqual(["blob:a"]);
    expect(cache.acquire("a")).toBeNull();
    expect(cache.bytes).toBe(60);
    expect(cache.acquire("b")).toBe("blob:b");
  });

  it("keeps an unreferenced entry while it still fits the budget", () => {
    const { cache, revoked } = makeCache(100);
    cache.insert("a", "blob:a", 40);
    cache.release("blob:a");

    // Under budget: retained for reuse, not revoked.
    expect(revoked).toEqual([]);
    expect(cache.acquire("a")).toBe("blob:a");
  });

  it("evicts least-recently-used first", () => {
    const { cache, revoked } = makeCache(100);
    cache.insert("a", "blob:a", 30);
    cache.release("blob:a");
    cache.insert("b", "blob:b", 30);
    cache.release("blob:b");

    // "a" is the oldest unreferenced entry, so it goes first.
    cache.insert("c", "blob:c", 50);
    expect(revoked).toEqual(["blob:a"]);
    expect(cache.bytes).toBe(80);

    // Touching "b" makes it most-recently-used, so the next squeeze spares it.
    cache.acquire("b");
    cache.release("blob:b");
    cache.insert("d", "blob:d", 40);
    expect(revoked).toEqual(["blob:a", "blob:b"]);
    expect(cache.acquire("b")).toBeNull();
    expect(cache.acquire("c")).toBe("blob:c");
  });

  it("reports urls it does not own so the caller can revoke them itself", () => {
    const { cache, revoked } = makeCache(100);
    cache.insert("a", "blob:a", 10);
    expect(cache.release("blob:unknown")).toBe(false);
    expect(revoked).toEqual([]);
  });

  it("revokes a replaced url when the same path is inserted twice", () => {
    const { cache, revoked } = makeCache(1000);
    cache.insert("a", "blob:old", 10);
    cache.insert("a", "blob:new", 20);

    expect(revoked).toEqual(["blob:old"]);
    expect(cache.bytes).toBe(20);
    expect(cache.acquire("a")).toBe("blob:new");
    expect(cache.release("blob:old")).toBe(false);
  });

  it("does not drive the reference count below zero", () => {
    const { cache } = makeCache(1000);
    cache.insert("a", "blob:a", 10);
    cache.release("blob:a");
    cache.release("blob:a");
    expect(cache.refCount("a")).toBe(0);
  });
});
