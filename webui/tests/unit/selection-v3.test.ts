import { describe, expect, it } from "vitest";

import type { EventPayloadDto, NodeDto, NodeSelectionDto } from "@/model/dto";
import { applyRuntimeEvent } from "@/runtime/event-state";
import { createRuntimeStore } from "@/runtime/store";

const selection = (activeNodeId: string | undefined): NodeSelectionDto => ({
  version: 2,
  intent: { mode: "auto" },
  activeTerminal: activeNodeId ? { kind: "node", nodeId: activeNodeId } : { kind: "unresolved", reason: "active_node_unresolved" },
  changedAt: 1,
});

const event = (kind: EventPayloadDto["kind"], value: Readonly<Record<string, unknown>>): EventPayloadDto => ({ kind, value });

describe("Protocol v5 selection runtime", () => {
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
    expect(store.selection.value?.activeTerminal).toEqual({ kind: "node", nodeId: nodes[9_999]!.id });
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
      report: { status: "partial", trigger: "manual", generation: 7, bootstrap_ms: 1, elapsed_ms: 100, timing: { thread_spawn_us: 100, runtime_init_us: 900, candidate_dispatch_us: 200, probe_us: 98_000, result_assembly_us: 300, total_us: 100_000 }, probe: { first_result_us: 90_000, last_result_us: 90_000, last_success_us: 90_000, completed_within_500ms: 1, completed_within_1s: 1, completed_within_2s: 1, completed_within_3s: 1, completed_before_cutoff: 1, cutoff_pending: 1, cutoff_tail_us: 10_000 }, tested: 2, succeeded: 1, timed_out: 1, failed: 0, nodes: [
        { node_id: first.id, state: "timeout", request_elapsed_us: 98_000, completed_at_us: 100_000 },
        { node_id: second.id, state: "success", latency_ms: 42, request_elapsed_us: 80_000, completed_at_us: 90_000 },
      ] },
      selection: { version: 2, intent: { mode: "auto" }, active_terminal: { kind: "node", node_id: second.id }, changed_at: 2 },
      fast_selection: { state: "not_needed", completed: 2, candidate_count: 2, elapsed_us: 100_000 },
      timing: {
        admission_us: 500,
        worker_reap_us: 200,
        fast_control: { intent_load_us: 0, current_snapshot_us: 0, decision_us: 0, target_resolve_us: 0, selector_apply_us: 0, final_snapshot_us: 0, total_us: 0 },
        terminal_control: { intent_load_us: 50, current_snapshot_us: 0, decision_us: 0, target_resolve_us: 0, selector_apply_us: 0, final_snapshot_us: 250, total_us: 400 },
        operation_total_us: 101_100,
      },
    };
    expect(applyRuntimeEvent(event("node_test", { result }), store)).toBe("applied");
    expect(store.nodesById.value[first.id]).not.toBe(firstBefore);
    expect(store.nodesById.value[first.id]?.latencyMs).toBeUndefined();
    expect(store.nodesById.value[second.id]).not.toBe(secondBefore);
    expect(store.nodesById.value[second.id]?.latencyMs).toBe(42);
    expect(store.nodeOrder.value).toBe(orderBefore);
    expect(store.selection.value?.activeTerminal).toEqual({ kind: "node", nodeId: second.id });
  });

  it("applies one benchmark progress outcome without waiting for the terminal report", () => {
    const store = createRuntimeStore();
    const first: NodeDto = { id: "nh1s-0000000000000001", name: "A", protocol: "vless", isRequested: false, isActive: true, sourceIds: ["src_a"] };
    const second: NodeDto = { id: "nh1s-0000000000000002", name: "B", protocol: "trojan", isRequested: false, isActive: false, sourceIds: ["src_a"] };
    store.loadNodeSnapshot({ nodes: [first, second], selection: selection(first.id) });
    store.beginNodeBenchmark([first.id, second.id]);
    const result = {
      operation_id: `bench_${"2".repeat(29)}`,
      phase: "progress",
      generation: 7,
      completed: 1,
      candidate_count: 2,
      outcome: { node_id: second.id, state: "success", latency_ms: 42, request_elapsed_us: 40_000, completed_at_us: 41_000 },
    };
    expect(applyRuntimeEvent(event("node_test", { result }), store)).toBe("applied");
    expect(store.nodesById.value[second.id]?.latencyMs).toBe(42);
    expect(store.nodeProbeStates.value[second.id]).toBe("success");
    expect(store.nodeProbeStates.value[first.id]).toBe("measuring");
  });

  it("switches immediately at the fast-selection milestone and keeps pending probes", () => {
    const store = createRuntimeStore();
    const first: NodeDto = { id: "nh1s-0000000000000001", name: "A", protocol: "vless", isRequested: false, isActive: true, sourceIds: ["src_a"] };
    const second: NodeDto = { id: "nh1s-0000000000000002", name: "B", protocol: "trojan", isRequested: false, isActive: false, sourceIds: ["src_a"] };
    store.loadNodeSnapshot({ nodes: [first, second], selection: selection(first.id) });
    store.beginNodeBenchmark([first.id, second.id]);

    const result = {
      operation_id: `bench_${"3".repeat(29)}`,
      phase: "selection",
      generation: 7,
      fast_selection: {
        state: "switched",
        completed: 1,
        candidate_count: 2,
        elapsed_us: 2_100_000,
        selection: { version: 2, intent: { mode: "auto" }, active_terminal: { kind: "node", node_id: second.id }, changed_at: 2 },
      },
    };

    expect(applyRuntimeEvent(event("node_test", { result }), store)).toBe("applied");
    expect(store.selection.value?.activeTerminal).toEqual({ kind: "node", nodeId: second.id });
    expect(store.nodeBenchmarkFastSelection.value).toMatchObject({ state: "switched", completed: 1, candidateCount: 2 });
    expect(store.nodeProbeStates.value[first.id]).toBe("measuring");
    expect(store.nodeProbeStates.value[second.id]).toBe("measuring");
  });

  it("does not fabricate a selection for a non-switching milestone", () => {
    const store = createRuntimeStore();
    const node: NodeDto = { id: "nh1s-0000000000000001", name: "A", protocol: "vless", isRequested: false, isActive: true, sourceIds: ["src_a"] };
    const original = selection(node.id);
    store.loadNodeSnapshot({ nodes: [node], selection: original });
    store.beginNodeBenchmark([node.id]);

    expect(applyRuntimeEvent(event("node_test", { result: {
      operation_id: `bench_${"4".repeat(29)}`,
      phase: "selection",
      generation: 7,
      fast_selection: { state: "kept", completed: 1, candidate_count: 1, elapsed_us: 2_000_000 },
    } }), store)).toBe("applied");
    expect(store.selection.value).toBe(original);
    expect(store.nodeBenchmarkFastSelection.value?.state).toBe("kept");
  });

  it("treats generation-affecting events as explicit reload boundaries", () => {
    const store = createRuntimeStore();
    expect(applyRuntimeEvent(event("generation", { generation: 8 }), store)).toBe("reload");
    expect(applyRuntimeEvent(event("subscription_mode", { mode: "merge" }), store)).toBe("reload");
  });
});
