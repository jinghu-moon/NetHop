import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-vue";

import ActiveNodeSummary from "@/components/nodes/ActiveNodeSummary.vue";
import NodeCard from "@/components/nodes/NodeCard.vue";
import TerritoryFlag from "@/components/nodes/TerritoryFlag.vue";
import type { NodeDto } from "@/model/dto";
import type { ActiveNodeView } from "@/model/node-view";
import "@/styles/base.css";

const node: NodeDto = {
  id: "nh1s-0123456789abcdef",
  name: "东京 · 极长名称用于验证固定卡片布局不会发生跳动",
  protocol: "vless",
  latencyMs: 119,
  alive: true,
  isRequested: false,
  isActive: true,
  sourceIds: ["src_a"],
  displayTerritoryCode: "JP",
};

describe("node territory components", () => {
  it("maps known territory codes to local SVG and unknown values to a stable globe", async () => {
    render({ components: { TerritoryFlag }, template: `<TerritoryFlag code="JP"/><TerritoryFlag/>` });
    const flags = document.querySelectorAll<HTMLElement>(".territory-flag");
    expect(flags).toHaveLength(2);
    await expect.element(flags[0]!.querySelector<HTMLImageElement>("img")).toBeVisible();
    await expect.element(flags[1]!.querySelector<SVGElement>("svg")).toBeVisible();
    expect(flags[0]!.querySelector("img")?.getAttribute("src")).not.toMatch(/^https?:/);
  });

  it("keeps active and requested states distinct with fixed geometry", async () => {
    render({ components: { NodeCard }, data: () => ({ active: node, pending: { ...node, id: "nh1s-fedcba9876543210", isActive: false, isRequested: true, latencyMs: 250 } }), template: `<div class="node-grid-row"><NodeCard :node="active"/><NodeCard :node="pending"/></div>` });
    const cards = document.querySelectorAll<HTMLElement>(".node-card");
    expect(cards).toHaveLength(2);
    expect(cards[0]!.dataset.active).toBe("true");
    expect(cards[0]!.dataset.requested).toBe("false");
    expect(cards[1]!.dataset.active).toBe("false");
    expect(cards[1]!.dataset.requested).toBe("true");
    expect(cards[0]!.getBoundingClientRect().height).toBe(72);
    expect(cards[1]!.getBoundingClientRect().height).toBe(72);
    expect(cards[0]!.querySelector(".node-card-latency")?.getAttribute("data-state")).toBe("good");
    expect(cards[1]!.querySelector(".node-card-latency")?.getAttribute("data-state")).toBe("poor");
  });

  it("renders node and non-node active terminal projections without fallback", async () => {
    const active: ActiveNodeView = { kind: "node", node, modeLabel: "自动优选", sourceLabel: "Primary +1", latency: { state: "good", text: "119 ms" } };
    const direct: ActiveNodeView = { kind: "direct", title: "当前直连", detail: "流量未经过代理节点" };
    const screen = render({ components: { ActiveNodeSummary }, data: () => ({ active, direct }), template: `<ActiveNodeSummary :value="active"/><ActiveNodeSummary :value="direct"/>` });
    await expect.element(screen.getByText(node.name)).toBeVisible();
    await expect.element(screen.getByText("Primary +1", { exact: false })).toBeVisible();
    await expect.element(screen.getByText("当前直连")).toBeVisible();
    expect(document.querySelectorAll(".active-node-summary")).toHaveLength(2);
  });
});
