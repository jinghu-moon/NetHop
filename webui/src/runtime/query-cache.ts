export interface QueryCacheOptions { readonly capacity?: number; readonly ttlMs?: number; readonly now?: () => number }

interface Entry<T> { readonly value: T; readonly expiresAt: number; readonly touchedAt: number }

export class BoundedQueryCache<T> {
  private readonly entries = new Map<string, Entry<T>>();
  private readonly capacity: number;
  private readonly ttlMs: number;
  private readonly now: () => number;
  private touchSequence = 0;
  constructor(options: QueryCacheOptions = {}) {
    this.capacity = Math.max(1, Math.min(options.capacity ?? 8, 32));
    this.ttlMs = Math.max(1_000, Math.min(options.ttlMs ?? 30_000, 300_000));
    this.now = options.now ?? (() => Date.now());
  }
  get(key: string): T | undefined {
    const entry = this.entries.get(key);
    if (!entry || entry.expiresAt <= this.now()) { this.entries.delete(key); return undefined; }
    this.entries.set(key, { ...entry, touchedAt: ++this.touchSequence });
    return entry.value;
  }
  set(key: string, value: T): void {
    const now = this.now();
    this.entries.set(key, { value, expiresAt: now + this.ttlMs, touchedAt: ++this.touchSequence });
    while (this.entries.size > this.capacity) {
      const oldest = [...this.entries.entries()].sort(([, a], [, b]) => a.touchedAt - b.touchedAt)[0];
      if (!oldest) break;
      this.entries.delete(oldest[0]);
    }
  }
  clear(): void { this.entries.clear(); }
  size(): number { return this.entries.size; }
}
