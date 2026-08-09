import { describe, expect, it } from "vitest";
import { parseNodeSort, sortNodes } from "@/model/node-view";
import type { NodeDto } from "@/model/dto";

const nodes: NodeDto[] = [
  { id: "nh1s-0000000000000001", name: "B", protocol: "vless", selected: false, sourceIds: ["a"], latencyMs: 90 },
  { id: "nh1s-0000000000000002", name: "A", protocol: "trojan", selected: true, sourceIds: ["a"], latencyMs: 120 },
  { id: "nh1s-0000000000000003", name: "C", protocol: "vmess", selected: false, sourceIds: ["b"] },
];

describe("node view ordering", () => {
  it("keeps source order by default and puts the selected node first for explicit sorts", () => {
    expect(sortNodes(nodes, "default").map((node) => node.name)).toEqual(["B", "A", "C"]);
    expect(sortNodes(nodes, "name").map((node) => node.name)).toEqual(["A", "B", "C"]);
    expect(sortNodes(nodes, "latency").map((node) => node.name)).toEqual(["A", "B", "C"]);
  });
  it("falls back to default for unknown persisted values", () => expect(parseNodeSort("other")).toBe("default"));
});
