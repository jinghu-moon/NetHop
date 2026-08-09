export interface SearchEntry { readonly id: string; readonly searchText: string }

function normalize(value: string): string {
  return value.normalize("NFKC").trim().toLocaleLowerCase();
}

export class SearchIndex {
  private readonly entries = new Map<string, SearchEntry>();
  upsert(id: string, ...fields: readonly string[]): void { this.entries.set(id, { id, searchText: normalize(fields.join(" ")) }); }
  remove(id: string): void { this.entries.delete(id); }
  query(value: string, limit = 128): readonly string[] {
    const term = normalize(value);
    if (!term) return [...this.entries.keys()].slice(0, limit);
    return [...this.entries.values()].filter((entry) => entry.searchText.includes(term)).slice(0, Math.max(1, Math.min(limit, 128))).map((entry) => entry.id);
  }
  clear(): void { this.entries.clear(); }
  size(): number { return this.entries.size; }
}
