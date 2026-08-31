/**
 * Byte-budgeted, reference-counted LRU for video object URLs.
 *
 * Desktop video previews currently hold the entire file in memory as a Blob,
 * so this budget is what bounds app memory on media-heavy chats. A mounted
 * <video> holds one reference per acquire(); release() drops it. Entries with
 * zero references are evicted least-recently-used once the byte budget is
 * exceeded — only then is the injected revoke callback actually invoked.
 *
 * Pure data structure (revocation is injected) so it is unit-testable.
 */

interface VideoCacheEntry {
  url: string;
  bytes: number;
  refs: number;
}

export class VideoLruCache {
  /** path -> entry; Map insertion order doubles as the LRU order. */
  private entries = new Map<string, VideoCacheEntry>();
  private urlToPath = new Map<string, string>();
  private totalBytes = 0;

  constructor(
    private readonly budgetBytes: number,
    private readonly revoke: (url: string) => void,
  ) {}

  get size(): number {
    return this.entries.size;
  }

  get bytes(): number {
    return this.totalBytes;
  }

  refCount(path: string): number {
    return this.entries.get(path)?.refs ?? 0;
  }

  /** Look up an entry, take a reference, and mark it most-recently-used. */
  acquire(path: string): string | null {
    const entry = this.entries.get(path);
    if (!entry) return null;
    this.entries.delete(path);
    this.entries.set(path, entry);
    entry.refs += 1;
    return entry.url;
  }

  /**
   * Insert a freshly created URL with one reference held by the caller.
   * If the path was somehow loaded twice, the previous URL is revoked.
   */
  insert(path: string, url: string, bytes: number): void {
    const existing = this.entries.get(path);
    if (existing) {
      this.entries.delete(path);
      this.urlToPath.delete(existing.url);
      this.totalBytes -= existing.bytes;
      this.revoke(existing.url);
    }
    this.entries.set(path, { url, bytes, refs: 1 });
    this.urlToPath.set(url, path);
    this.totalBytes += bytes;
    this.evictOverBudget();
  }

  /**
   * Drop one reference by URL. Returns false for URLs this cache does not
   * own (caller should handle those itself). Unreferenced entries stay
   * cached for reuse until the byte budget forces eviction.
   */
  release(url: string): boolean {
    const path = this.urlToPath.get(url);
    if (path === undefined) return false;
    const entry = this.entries.get(path);
    if (!entry) return false;
    entry.refs = Math.max(0, entry.refs - 1);
    this.evictOverBudget();
    return true;
  }

  private evictOverBudget(): void {
    if (this.totalBytes <= this.budgetBytes) return;
    for (const [path, entry] of this.entries) {
      if (this.totalBytes <= this.budgetBytes) break;
      if (entry.refs > 0) continue;
      this.entries.delete(path);
      this.urlToPath.delete(entry.url);
      this.totalBytes -= entry.bytes;
      this.revoke(entry.url);
    }
  }
}

/** 512 MB — a handful of typical phone-camera videos. */
export const VIDEO_CACHE_BUDGET_BYTES = 512 * 1024 * 1024;
