import { describe, expect, it } from "vitest";

import type { EventPayloadDto, NodeDto, NodeSelectionDto } from "@/model/dto";
import { applyRuntimeEvent } from "@/runtime/event-state";
import { createRuntimeStore } from "@/runtime/store";

const selection = (activeNodeId: string | undefined): NodeSelectionDto => ({
  version: 1,
  intent: { mode: "auto" },
  ...(activeNodeId ? { activeNodeId } : {}),
  changedAt: 1,
});

const event = (kind: EventPayloadDto["kind"], value: Readonly<Record<string, unknown>>): EventPayloadDto => ({ kind, value });

describe("Protocol v3 selection runtime", () => {
  it("keeps subscription mode, active set and digest as independent daemon facts", () => {
    const store = createRuntimeStore();
    store.loadSubscriptionSnapshot({
      mode: "merge",
      activeSourceIds: ["src_a", "src_b"],
      configDigest: "a".repeat(64),
      sources: [
        { id: "src_a", name: "Primary", configured: true, active: true },
        { id: "src_b", name: "Backup", configured: true, active: true },
      ],
    });
    expect(store.subscriptionMode.value).toBe("merge");
    expect(store.activeSourceIds.value).toEqual(["src_a", "src_b"]);
    expect(store.subscriptionConfigDigest.value).toBe("a".repeat(64));
    expect(store.subscriptionsById.value.src_a?.active).toBe(true);
    expect(store.subscriptionsById.value.src_b?.active).toBe(true);
  });

  it("updates selection without rebuilding the 10,000-node entity table", () => {
    const store = createRuntimeStore();
    const nodes: NodeDto[] = Array.from({ length: 10_000 }, (_, index) => ({
      id: `nh1s-${index.toString(16).padStart(16, "0")}`,
      name: `Node ${index}`,
      protocol: "vless",
      isRequested: false,
      isActive: index === 0,
      sourceIds: ["src_a"],
    }));
    const started = performance.now();
    store.loadNodeSnapshot({ nodes, selection: selection(nodes[0]!.id) });
    const table = store.nodesById.value;
    const order = store.nodeOrder.value;
    store.setSelection(selection(nodes[9_999]!.id));
    expect(store.nodesById.value).toBe(table);
    expect(store.nodeOrder.value).toBe(order);
    expect(store.selection.value?.activeNodeId).toBe(nodes[9_999]!.id);
    expect(performance.now() - started).toBeLessThan(1_000);
  });

  it("applies one terminal report atomically and clears failed stale delays", () => {
    const store = createRuntimeStore();
    const first: NodeDto = { id: "nh1s-0000000000000001", name: "A", protocol: "vless", latencyMs: 999, isRequested: false, isActive: true, sourceIds: ["src_a"] };
    const second: NodeDto = { id: "nh1s-0000000000000002", name: "B", protocol: "trojan", isRequested: false, isActive: false, sourceIds: ["src_a"] };
    store.loadNodeSnapshot({ nodes: [first, second], selection: selection(first.id) });
    const firstBefore = store.nodesById.value[first.id];
    const secondBefore = store.nodesById.value[second.id];
    const orderBefore = store.nodeOrder.value;

    const result = {
      operation_id: `bench_${"1".repeat(29)}`,
      phase: "completed",
      report: { status: "partial", trigger: "manual", generation: 7, bootstrap_ms: 1, elapsed_ms: 100, tested: 2, succeeded: 1, timed_out: 1, failed: 0, nodes: [
        { node_id: first.id, state: "timeout" },
        { node_id: second.id, state: "success", latency_ms: 42 },
      ] },
      selection: { version: 1, intent: { mode: "auto" }, active_node_id: second.id, changed_at: 2 },
    };
    expect(applyRuntimeEvent(event("node_test", { result }), store)).toBe("applied");
    expect(store.nodesById.value[first.id]).not.toBe(firstBefore);
    expect(store.nodesById.value[first.id]?.latencyMs).toBeUndefined();
    expect(store.nodesById.value[second.id]).not.toBe(secondBefore);
    expect(store.nodesById.value[second.id]?.latencyMs).toBe(42);
    expect(store.nodeOrder.value).toBe(orderBefore);
    expect(store.selection.value?.activeNodeId).toBe(second.id);
  });

  it("treats generation-affecting events as explicit reload boundaries", () => {
    const store = createRuntimeStore();
    expect(applyRuntimeEvent(event("generation", { generation: 8 }), store)).toBe("reload");
    expect(applyRuntimeEvent(event("subscription_mode", { mode: "merge" }), store)).toBe("reload");
  });
});
