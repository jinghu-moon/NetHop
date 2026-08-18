import { describe, expect, it } from "vitest";

import { createConfigDraftStore, createRuntimeStore, createSessionStore } from "@/runtime/store";
import { canTransition, createActionLock, createOperationStore, reduceOperation } from "@/runtime/operation";
import { BoundedQueryCache } from "@/runtime/query-cache";
import { isAllowedUiStorageKey, uiStorageKey } from "@/runtime/storage";
import { SearchIndex } from "@/runtime/search-index";
import type { ConfigDto, NodeDto } from "@/model/dto";

const node = (id: string): NodeDto => ({ id, name: `Node ${id}`, protocol: "vless", isRequested: false, isActive: false, sourceIds: ["source"] });
const config = (document: Readonly<Record<string, unknown>>): ConfigDto => ({
  observedConfigDigest: "a".repeat(64), activeConfigDigest: "b".repeat(64), candidateSequence: 1,
  document, sourceStatus: [], sourceHistory: [],
});

describe("runtime stores", () => {
  it("tracks session phases and resets all connection state", () => {
    const store = createSessionStore();
    store.setPhase("connecting");
    store.setPhase("live");
    expect(store.phase.value).toBe("live");
    store.reset();
    expect(store.phase.value).toBe("idle");
    expect(store.hello.value).toBeUndefined();
  });

  it("normalizes entities while preserving stable order and replacing the root immutably", () => {
    const store = createRuntimeStore();
    store.replaceNodes([node("one"), node("two")]);
    const firstRoot = store.nodesById.value;
    store.upsertNode({ ...node("one"), isRequested: true });
    expect(store.nodeOrder.value).toEqual(["one", "two"]);
    expect(store.nodesById.value).not.toBe(firstRoot);
    expect(store.nodesById.value.one?.isRequested).toBe(true);
    store.removeNode("two");
    expect(store.nodeOrder.value).toEqual(["one"]);
  });

  it("keeps config draft isolated and supports discard/markApplied", () => {
    const store = createConfigDraftStore();
    const active = config({ service: { enabled: true } });
    store.load(active);
    store.edit({ service: { enabled: false } });
    expect(store.dirty.value).toBe(true);
    expect(store.active.value?.document).toEqual({ service: { enabled: true } });
    store.discard();
    expect(store.dirty.value).toBe(false);
    store.markApplied(config({ service: { enabled: false } }));
    expect(store.baseDigest.value).toBe("b".repeat(64));
  });
});

describe("operation state", () => {
  it("allows only forward transitions and terminal states stay terminal", () => {
    expect(canTransition("accepted", "running")).toBe(true);
    expect(canTransition("success", "running")).toBe(false);
    const current = { id: "1", key: "service", phase: "success" as const };
    expect(reduceOperation(current, { phase: "running" })).toBe(current);
    const store = createOperationStore();
    store.begin("1", "service");
    store.update("1", "running");
    store.update("1", "success");
    expect(store.active.value).toHaveLength(0);
    expect(store.byId["1"]?.phase).toBe("success");
  });

  it("locks the same key but permits different keys", async () => {
    const lock = createActionLock();
    let release!: () => void;
    const first = lock.run("source:a", () => new Promise<void>((resolve) => { release = resolve; }));
    await expect(lock.run("source:a", async () => undefined)).rejects.toThrow("already running");
    expect(lock.has("source:a")).toBe(true);
    await expect(lock.run("source:b", async () => "ok")).resolves.toBe("ok");
    release();
    await first;
    expect(lock.has("source:a")).toBe(false);
  });
});

describe("bounded query helpers", () => {
  it("evicts least recently touched entries and expires by TTL", () => {
    let now = 0;
    const cache = new BoundedQueryCache<string>({ capacity: 2, ttlMs: 1000, now: () => now });
    cache.set("a", "A"); cache.set("b", "B");
    expect(cache.get("a")).toBe("A");
    cache.set("c", "C");
    expect(cache.get("b")).toBeUndefined();
    now = 1001;
    expect(cache.get("a")).toBeUndefined();
    expect(cache.size()).toBe(1);
  });

  it("allows only namespaced UI storage keys", () => {
    expect(uiStorageKey("theme")).toBe("nethop.ui.theme");
    expect(isAllowedUiStorageKey("nethop.ui.last-route")).toBe(true);
    expect(isAllowedUiStorageKey("nethop.config")).toBe(false);
    expect(() => uiStorageKey("config" as never)).toThrow();
  });

  it("normalizes search text and bounds results", () => {
    const index = new SearchIndex();
    for (let i = 0; i < 140; i += 1) index.upsert(`id-${i}`, `  Nódé ${i}  `);
    expect(index.query("NÓDÉ", 8)).toHaveLength(8);
    index.remove("id-0");
    expect(index.query("nódé").includes("id-0")).toBe(false);
  });
});
