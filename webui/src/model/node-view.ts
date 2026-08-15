import type { NodeDto, NodeProbeStateDto, NodeSelectionDto } from "./dto";

export type NodeSort = "default" | "name" | "latency-asc" | "latency-desc";
export type NodeLatencyState = "good" | "medium" | "poor" | "unknown" | "measuring" | NodeProbeStateDto;

export interface NodeLatencyView {
  readonly state: NodeLatencyState;
  readonly text: string;
}

export type ActiveNodeView =
  | { readonly kind: "node"; readonly node: NodeDto; readonly modeLabel: string; readonly sourceLabel: string; readonly latency: NodeLatencyView }
  | { readonly kind: "direct"; readonly title: "当前直连"; readonly detail: "流量未经过代理节点" }
  | { readonly kind: "block"; readonly title: "当前阻断"; readonly detail: "流量由阻断出口处理" }
  | { readonly kind: "unresolved"; readonly title: "活动节点不可用"; readonly detail: string }
  | { readonly kind: "syncing"; readonly title: "正在同步活动节点"; readonly detail: "等待最新节点快照" }
  | { readonly kind: "stopped"; readonly title: "代理未运行"; readonly detail: "当前没有活动代理节点" };

const collator = new Intl.Collator("zh-CN", { numeric: true, sensitivity: "base" });

export function sortNodes(nodes: readonly NodeDto[], sort: NodeSort): readonly NodeDto[] {
  if (sort === "default") return [...nodes];
  return [...nodes].sort((left, right) => {
    if (sort === "name") return collator.compare(left.name, right.name) || left.id.localeCompare(right.id);
    const leftKnown = left.latencyMs !== undefined;
    const rightKnown = right.latencyMs !== undefined;
    if (leftKnown !== rightKnown) return leftKnown ? -1 : 1;
    const direction = sort === "latency-desc" ? -1 : 1;
    const byDelay = leftKnown && rightKnown ? (left.latencyMs! - right.latencyMs!) * direction : 0;
    return byDelay || collator.compare(left.name, right.name) || left.id.localeCompare(right.id);
  });
}

export function parseNodeSort(value: string): NodeSort {
  return value === "name" || value === "latency-asc" || value === "latency-desc" ? value : "default";
}

export function activeNodeId(selection: NodeSelectionDto | undefined): string | undefined {
  return selection?.activeTerminal.kind === "node" ? selection.activeTerminal.nodeId : undefined;
}

export function latencyView(latencyMs?: number, probeState?: NodeProbeStateDto | "measuring"): NodeLatencyView {
  if (probeState === "measuring") return { state: "measuring", text: "···" };
  if (probeState === "timeout") return { state: "timeout", text: "超时" };
  if (probeState === "unavailable" || probeState === "protocol_error") return { state: probeState, text: "不可用" };
  if (latencyMs === undefined) return { state: "unknown", text: "--" };
  if (latencyMs < 120) return { state: "good", text: `${latencyMs} ms` };
  if (latencyMs < 250) return { state: "medium", text: `${latencyMs} ms` };
  return { state: "poor", text: `${latencyMs} ms` };
}

export function activeNodeView(
  selection: NodeSelectionDto | undefined,
  nodesById: Readonly<Record<string, NodeDto>>,
  sourceNames: Readonly<Record<string, string>>,
  running = true,
): ActiveNodeView {
  if (!running) return { kind: "stopped", title: "代理未运行", detail: "当前没有活动代理节点" };
  if (!selection) return { kind: "syncing", title: "正在同步活动节点", detail: "等待最新节点快照" };
  const terminal = selection.activeTerminal;
  if (terminal.kind === "direct") return { kind: "direct", title: "当前直连", detail: "流量未经过代理节点" };
  if (terminal.kind === "block") return { kind: "block", title: "当前阻断", detail: "流量由阻断出口处理" };
  if (terminal.kind === "unresolved") return { kind: "unresolved", title: "活动节点不可用", detail: terminal.reason };
  const node = nodesById[terminal.nodeId];
  if (!node) return { kind: "syncing", title: "正在同步活动节点", detail: "等待最新节点快照" };
  const labels = node.sourceIds.map((sourceId) => sourceNames[sourceId]).filter((label): label is string => Boolean(label));
  const sourceLabel = labels.length > 1 ? `${labels[0]} +${labels.length - 1}` : labels[0] ?? "未知来源";
  return {
    kind: "node",
    node,
    modeLabel: selection.intent.mode === "auto" ? "自动优选" : "手动选择",
    sourceLabel,
    latency: latencyView(node.latencyMs, node.alive === false ? "unavailable" : undefined),
  };
}
