import { describe, expect, it } from "vitest";
import { activeNodeView, latencyView, parseNodeSort, sortNodes } from "@/model/node-view";
import type { NodeDto } from "@/model/dto";

const nodes: NodeDto[] = [
  { id: "nh1s-0000000000000001", name: "B", protocol: "vless", isRequested: false, isActive: false, sourceIds: ["a"], latencyMs: 90 },
  { id: "nh1s-0000000000000002", name: "A", protocol: "trojan", isRequested: true, isActive: true, sourceIds: ["a"], latencyMs: 120 },
  { id: "nh1s-0000000000000003", name: "C", protocol: "vmess", isRequested: false, isActive: false, sourceIds: ["b"] },
];

describe("node view ordering", () => {
  it("keeps source order by default and applies strict explicit sorts", () => {
    expect(sortNodes(nodes, "default").map((node) => node.name)).toEqual(["B", "A", "C"]);
    expect(sortNodes(nodes, "name").map((node) => node.name)).toEqual(["A", "B", "C"]);
    expect(sortNodes(nodes, "latency-asc").map((node) => node.name)).toEqual(["B", "A", "C"]);
    expect(sortNodes(nodes, "latency-desc").map((node) => node.name)).toEqual(["A", "B", "C"]);
  });
  it("parses both latency directions and falls back for unknown persisted values", () => {
    expect(parseNodeSort("latency-asc")).toBe("latency-asc");
    expect(parseNodeSort("latency-desc")).toBe("latency-desc");
    expect(parseNodeSort("other")).toBe("default");
  });

  it("keeps unknown latency last in both directions", () => {
    expect(sortNodes(nodes, "latency-asc").map((node) => node.name)).toEqual(["B", "A", "C"]);
    expect(sortNodes(nodes, "latency-desc").map((node) => node.name)).toEqual(["A", "B", "C"]);
  });

  it("freezes latency tier boundaries and terminal labels", () => {
    expect([119, 120, 249, 250].map((value) => latencyView(value).state)).toEqual(["good", "medium", "medium", "poor"]);
    expect(latencyView(undefined, "measuring")).toEqual({ state: "measuring", text: "···" });
    expect(latencyView(undefined, "timeout")).toEqual({ state: "timeout", text: "超时" });
    expect(latencyView(undefined, "protocol_error")).toEqual({ state: "protocol_error", text: "不可用" });
  });

  it("never falls back to the first node while the active node is syncing", () => {
    const syncing = activeNodeView({ version: 2, intent: { mode: "auto" }, activeTerminal: { kind: "node", nodeId: "missing" }, changedAt: 1 }, { first: nodes[0]! }, {});
    expect(syncing.kind).toBe("syncing");
    expect(activeNodeView({ version: 2, intent: { mode: "auto" }, activeTerminal: { kind: "direct" }, changedAt: 1 }, {}, {}).kind).toBe("direct");
    expect(activeNodeView({ version: 2, intent: { mode: "auto" }, activeTerminal: { kind: "block" }, changedAt: 1 }, {}, {}).kind).toBe("block");
    expect(activeNodeView(undefined, {}, {}, false).kind).toBe("stopped");
  });
});
