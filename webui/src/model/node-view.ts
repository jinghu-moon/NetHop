import type { NodeDto } from "./dto";

export type NodeSort = "default" | "name" | "latency";

const collator = new Intl.Collator("zh-CN", { numeric: true, sensitivity: "base" });

export function sortNodes(nodes: readonly NodeDto[], sort: NodeSort): readonly NodeDto[] {
  if (sort === "default") return [...nodes];
  return [...nodes].sort((left, right) => {
    if (left.selected !== right.selected) return left.selected ? -1 : 1;
    if (sort === "name") return collator.compare(left.name, right.name) || left.id.localeCompare(right.id);
    const leftDelay = left.latencyMs ?? Number.POSITIVE_INFINITY;
    const rightDelay = right.latencyMs ?? Number.POSITIVE_INFINITY;
    return leftDelay - rightDelay || collator.compare(left.name, right.name) || left.id.localeCompare(right.id);
  });
}

export function parseNodeSort(value: string): NodeSort {
  return value === "name" || value === "latency" ? value : "default";
}
