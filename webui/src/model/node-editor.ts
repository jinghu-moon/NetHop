import { parseConfigDocument } from "./config-json";

const MAX_OUTBOUND_BYTES = 64 * 1024;
const STABLE_NODE_ID = /^nh1s-[a-f0-9]{16}$/;

export interface NodeOverrideDto {
  readonly nodeId: string;
  readonly overridden: boolean;
  readonly displayName: string;
  readonly outbound: Readonly<Record<string, unknown>>;
}

export function parseNodeOverride(value: unknown): NodeOverrideDto {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("节点编辑数据无效");
  const object = value as Record<string, unknown>;
  const allowed = new Set(["node_id", "overridden", "display_name", "outbound"]);
  if (Object.keys(object).some((key) => !allowed.has(key))) throw new Error("节点编辑数据包含未知字段");
  if (typeof object.node_id !== "string" || !STABLE_NODE_ID.test(object.node_id)) throw new Error("节点 ID 无效");
  if (typeof object.overridden !== "boolean") throw new Error("节点编辑状态无效");
  if (typeof object.display_name !== "string" || !validDisplayName(object.display_name)) throw new Error("节点名称无效");
  if (!object.outbound || typeof object.outbound !== "object" || Array.isArray(object.outbound)) throw new Error("节点 outbound 无效");
  return {
    nodeId: object.node_id,
    overridden: object.overridden,
    displayName: object.display_name,
    outbound: object.outbound as Readonly<Record<string, unknown>>,
  };
}

export function serializeNodeOutbound(outbound: Readonly<Record<string, unknown>>): string {
  return JSON.stringify(outbound, null, 2);
}

export function parseNodeOutbound(input: string): Readonly<Record<string, unknown>> {
  const bytes = new TextEncoder().encode(input);
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_OUTBOUND_BYTES) throw new Error("节点 outbound 大小无效");
  const object = parseConfigDocument(input);
  if (typeof object.type !== "string" || typeof object.server !== "string") throw new Error("节点 outbound 缺少协议或服务器");
  return object;
}

export function buildNodeOverridePayload(
  nodeId: string,
  displayName: string,
  outbound: Readonly<Record<string, unknown>>,
): string {
  if (!STABLE_NODE_ID.test(nodeId) || !validDisplayName(displayName)) throw new Error("节点编辑字段无效");
  return JSON.stringify({
    target: nodeId,
    node_override: {
      display_name: displayName,
      outbound,
    },
  });
}

function validDisplayName(value: string): boolean {
  return value.length > 0 && new TextEncoder().encode(value).byteLength <= 128 && !/[\u0000-\u001f\u007f]/.test(value);
}
