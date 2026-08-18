export const NETHOPCTL_PATH = "/data/adb/modules/nethop/bin/nethopctl";
export const PROTOCOL_VERSION = 6;
export const MAX_COMMAND_ARG_BYTES = 16 * 1024;
export const MAX_JSON_BYTES = 1024 * 1024;
export const EVENT_SESSION_MAX_RUNTIME_SECONDS = 300;

export type EventKind = "config" | "runtime" | "subscription" | "generation" | "network" | "traffic" | "subscription-mode" | "subscription-active-set" | "node-selection" | "node-active" | "node-test";
export type LogChannel = "service" | "subscription" | "core";
export interface PayloadOperationByNamespace {
  readonly config: "config-validate" | "config-apply" | "config-mutate";
  readonly subscription: "subscription-import-preview" | "subscription-import-apply";
  readonly backup: "backup-restore";
  readonly node: "node-override-apply";
}
export type PayloadNamespace = keyof PayloadOperationByNamespace;
export type PayloadOperation = PayloadOperationByNamespace[PayloadNamespace];

export type OperationRequest =
  | { readonly id: "hello"; readonly managerVersion: string }
  | { readonly id: "status.get" }
  | { readonly id: "service.start"; readonly wait?: boolean }
  | { readonly id: "service.stop"; readonly wait?: boolean }
  | { readonly id: "capability.get" }
  | { readonly id: "config.get" }
  | { readonly id: "config.schema" }
  | { readonly id: "traffic.get" }
  | { readonly id: "metrics.get" }
  | { readonly id: "config.reload" }
  | { readonly id: "events.subscribe"; readonly kinds: readonly EventKind[]; readonly sessionId: string }
  | { readonly id: "events.terminate"; readonly sessionId: string }
  | { readonly id: "node.list"; readonly query?: string; readonly limit?: number }
  | { readonly id: "node.test"; readonly nodeId: string }
  | { readonly id: "node.test-all" }
  | { readonly id: "node.selection.get" }
  | { readonly id: "node.select.auto" }
  | { readonly id: "node.select.manual"; readonly nodeId: string }
  | { readonly id: "node.export"; readonly nodeId: string }
  | { readonly id: "node.override.get"; readonly nodeId: string }
  | { readonly id: "node.override.remove"; readonly nodeId: string }
  | { readonly id: "node.remove"; readonly nodeId: string; readonly expectedDigest: string }
  | { readonly id: "subscription.list" }
  | { readonly id: "subscription.mode.get" }
  | { readonly id: "subscription.mode.set"; readonly mode: "single" | "merge"; readonly sourceId?: string; readonly expectedDigest: string }
  | { readonly id: "subscription.select"; readonly sourceId: string; readonly expectedDigest: string }
  | { readonly id: "subscription.set-enabled"; readonly sourceId: string; readonly enabled: boolean; readonly expectedDigest: string }
  | { readonly id: "subscription.update"; readonly sourceId?: string; readonly wait?: boolean }
  | { readonly id: "subscription.enable"; readonly sourceId: string; readonly expectedDigest: string }
  | { readonly id: "subscription.disable"; readonly sourceId: string; readonly expectedDigest: string }
  | { readonly id: "subscription.move"; readonly sourceId: string; readonly beforeSourceId?: string; readonly expectedDigest: string }
  | { readonly id: "subscription.remove"; readonly sourceId: string; readonly expectedDigest: string }
  | { readonly id: "application.list" }
  | { readonly id: "logs.get"; readonly channel?: LogChannel; readonly limit?: number }
  | { readonly id: "logs.clear" }
  | { readonly id: "connections.get"; readonly query?: string; readonly limit?: number }
  | { readonly id: "connection.close"; readonly connectionId: string }
  | { readonly id: "connections.close-all" }
  | { readonly id: "diagnostics.bundle" }
  | { readonly id: "topology.get" }
  | { readonly id: "ruleset.status" }
  | { readonly id: "ruleset.update"; readonly wait?: boolean }
  | { readonly id: "core.version-check" }
  | { readonly id: "backup.export" }
  | { readonly id: "webui.payload.create"; readonly namespace: PayloadNamespace }
  | { readonly id: "webui.payload.append"; readonly namespace: PayloadNamespace; readonly handle: string; readonly chunk: string }
  | { readonly id: "webui.payload.commit"; readonly namespace: PayloadNamespace; readonly handle: string; readonly operation: PayloadOperation }
  | { readonly id: "webui.payload.remove"; readonly namespace: PayloadNamespace; readonly handle: string };

export interface BuiltCommand {
  readonly executable: typeof NETHOPCTL_PATH;
  readonly args: readonly string[];
  readonly timeoutMs: number;
  readonly sensitive: boolean;
}

const SAFE_ID = /^[A-Za-z0-9_.:-]{1,256}$/;
const SAFE_HANDLE = /^p_[a-f0-9]{32}$/;
const SAFE_BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;
const SAFE_MANAGER_VERSION = /^[A-Za-z0-9][A-Za-z0-9.+_-]{0,63}$/;
const SAFE_EVENT_SESSION = /^evt_[a-f0-9]{32}$/;
const SAFE_SOURCE_ID = /^src_[a-f0-9]{32}$/;
const SAFE_DIGEST = /^[a-f0-9]{64}$/;
const SAFE_NODE_ID = /^nh1s-[a-f0-9]{16}$/;
const SAFE_CONNECTION_ID = /^[A-Za-z0-9_.:-]{1,256}$/;
const allowedKinds: readonly EventKind[] = ["config", "runtime", "subscription", "generation", "network", "traffic", "subscription-mode", "subscription-active-set", "node-selection", "node-active", "node-test"];
const payloadOperations: { readonly [Namespace in PayloadNamespace]: readonly PayloadOperationByNamespace[Namespace][] } = {
  config: ["config-validate", "config-apply", "config-mutate"],
  subscription: ["subscription-import-preview", "subscription-import-apply"],
  backup: ["backup-restore"],
  node: ["node-override-apply"],
};

function assertSafe(value: string, pattern: RegExp, label: string): void {
  if (!pattern.test(value) || /[\0\r\n;&|`$<>]/.test(value)) {
    throw new Error(`invalid ${label}`);
  }
}

function assertLimit(value: string, label: string): void {
  if (new TextEncoder().encode(value).byteLength > MAX_COMMAND_ARG_BYTES) throw new Error(`${label} exceeds command bound`);
}

function listArgs(query: string | undefined, limit: number | undefined): string[] {
  const args: string[] = ["--json"];
  if (query !== undefined) {
    assertSafe(query, SAFE_ID, "query");
    assertLimit(query, "query");
    args.push(query);
  }
  if (limit !== undefined) {
    if (!Number.isInteger(limit) || limit < 1 || limit > 128) throw new Error("invalid limit");
    args.push("--limit", String(limit));
  }
  return args;
}

export function buildCommand(request: OperationRequest): BuiltCommand {
  switch (request.id) {
    case "hello":
      assertSafe(request.managerVersion, SAFE_MANAGER_VERSION, "manager version");
      return { executable: NETHOPCTL_PATH, args: ["hello", "--json", "--manager-version", request.managerVersion, "--protocol-min", String(PROTOCOL_VERSION), "--protocol-max", String(PROTOCOL_VERSION)], timeoutMs: 5000, sensitive: false };
    case "status.get": return { executable: NETHOPCTL_PATH, args: ["status", "--json"], timeoutMs: 5000, sensitive: false };
    case "service.start": return { executable: NETHOPCTL_PATH, args: ["start", "--json", ...(request.wait ? ["--wait"] : [])], timeoutMs: request.wait ? 30000 : 5000, sensitive: false };
    case "service.stop": return { executable: NETHOPCTL_PATH, args: ["stop", "--json", ...(request.wait ? ["--wait"] : [])], timeoutMs: request.wait ? 30000 : 5000, sensitive: false };
    case "capability.get": return { executable: NETHOPCTL_PATH, args: ["capability", "get", "--json"], timeoutMs: 5000, sensitive: false };
    case "config.get": return { executable: NETHOPCTL_PATH, args: ["config", "get", "--json"], timeoutMs: 5000, sensitive: false };
    case "config.reload": return { executable: NETHOPCTL_PATH, args: ["config", "reload", "--json", "--wait"], timeoutMs: 30000, sensitive: false };
    case "config.schema": return { executable: NETHOPCTL_PATH, args: ["config", "schema", "--json"], timeoutMs: 5000, sensitive: false };
    case "traffic.get": return { executable: NETHOPCTL_PATH, args: ["traffic", "--json"], timeoutMs: 5000, sensitive: false };
    case "metrics.get": return { executable: NETHOPCTL_PATH, args: ["metrics", "--json"], timeoutMs: 5000, sensitive: false };
    case "events.subscribe": {
      const kinds = [...new Set(request.kinds)];
      if (kinds.some((kind) => !allowedKinds.includes(kind))) throw new Error("invalid event kind");
      assertSafe(request.sessionId, SAFE_EVENT_SESSION, "event session");
      return { executable: NETHOPCTL_PATH, args: ["events", "--jsonl", "--kinds", kinds.join(","), "--session-id", request.sessionId, "--max-runtime-seconds", String(EVENT_SESSION_MAX_RUNTIME_SECONDS)], timeoutMs: 0, sensitive: false };
    }
    case "events.terminate":
      assertSafe(request.sessionId, SAFE_EVENT_SESSION, "event session");
      return { executable: NETHOPCTL_PATH, args: ["webui", "events", "terminate", request.sessionId, "--json"], timeoutMs: 5000, sensitive: false };
    case "node.list": return { executable: NETHOPCTL_PATH, args: ["node", "list", ...listArgs(request.query, request.limit)], timeoutMs: 5000, sensitive: false };
    case "node.test":
      assertSafe(request.nodeId, SAFE_NODE_ID, "node id");
      return { executable: NETHOPCTL_PATH, args: ["node", "test", request.nodeId, "--json"], timeoutMs: 15000, sensitive: false };
    case "node.test-all":
      return { executable: NETHOPCTL_PATH, args: ["node", "test-all", "--json"], timeoutMs: 7000, sensitive: false };
    case "node.selection.get": return { executable: NETHOPCTL_PATH, args: ["node", "selection", "--json"], timeoutMs: 5000, sensitive: false };
    case "node.select.auto": return { executable: NETHOPCTL_PATH, args: ["node", "select", "auto", "--json"], timeoutMs: 5000, sensitive: false };
    case "node.select.manual":
      assertSafe(request.nodeId, SAFE_NODE_ID, "node id");
      return { executable: NETHOPCTL_PATH, args: ["node", "select", "manual", request.nodeId, "--json"], timeoutMs: 5000, sensitive: false };
    case "node.export":
      assertSafe(request.nodeId, SAFE_NODE_ID, "node id");
      return { executable: NETHOPCTL_PATH, args: ["node", "export", request.nodeId, "--json"], timeoutMs: 5000, sensitive: true };
    case "node.override.get":
      assertSafe(request.nodeId, SAFE_NODE_ID, "node id");
      return { executable: NETHOPCTL_PATH, args: ["node", "override", "get", request.nodeId, "--json"], timeoutMs: 5000, sensitive: true };
    case "node.override.remove":
      assertSafe(request.nodeId, SAFE_NODE_ID, "node id");
      return { executable: NETHOPCTL_PATH, args: ["node", "override", "remove", request.nodeId, "--json"], timeoutMs: 30000, sensitive: false };
    case "node.remove":
      assertSafe(request.nodeId, SAFE_NODE_ID, "node id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["node", "remove", request.nodeId, "--json", "--expected-digest", request.expectedDigest], timeoutMs: 30000, sensitive: false };
    case "subscription.list": return { executable: NETHOPCTL_PATH, args: ["subscription", "list", "--json"], timeoutMs: 5000, sensitive: false };
    case "subscription.mode.get": return { executable: NETHOPCTL_PATH, args: ["subscription", "mode", "--json"], timeoutMs: 5000, sensitive: false };
    case "subscription.mode.set": {
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      if (request.mode === "single") {
        if (!request.sourceId) throw new Error("single mode requires source id");
        assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      } else if (request.sourceId !== undefined) throw new Error("merge mode does not accept source id");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "mode", "set", request.mode, "--json", "--expected-digest", request.expectedDigest, ...(request.sourceId ? ["--source", request.sourceId] : [])], timeoutMs: 30000, sensitive: false };
    }
    case "subscription.select":
      assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "select", request.sourceId, "--json", "--expected-digest", request.expectedDigest], timeoutMs: 30000, sensitive: false };
    case "subscription.set-enabled":
      assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["subscription", request.enabled ? "enable" : "disable", request.sourceId, "--json", "--expected-digest", request.expectedDigest], timeoutMs: 30000, sensitive: false };
    case "subscription.update": {
      if (request.sourceId !== undefined) assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "update", "--json", "--wait", ...(request.sourceId ? ["--source", request.sourceId] : [])], timeoutMs: 30000, sensitive: false };
    }
    case "subscription.enable":
      assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "enable", request.sourceId, "--json", "--expected-digest", request.expectedDigest], timeoutMs: 30000, sensitive: false };
    case "subscription.disable":
      assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "disable", request.sourceId, "--json", "--expected-digest", request.expectedDigest], timeoutMs: 30000, sensitive: false };
    case "subscription.move":
      assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      if (request.beforeSourceId !== undefined) assertSafe(request.beforeSourceId, SAFE_SOURCE_ID, "source id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "move", request.sourceId, "--json", "--expected-digest", request.expectedDigest, ...(request.beforeSourceId ? ["--before", request.beforeSourceId] : [])], timeoutMs: 30000, sensitive: false };
    case "subscription.remove":
      assertSafe(request.sourceId, SAFE_SOURCE_ID, "source id");
      assertSafe(request.expectedDigest, SAFE_DIGEST, "config digest");
      return { executable: NETHOPCTL_PATH, args: ["subscription", "remove", request.sourceId, "--json", "--expected-digest", request.expectedDigest], timeoutMs: 30000, sensitive: false };
    case "application.list": return { executable: NETHOPCTL_PATH, args: ["application", "list", "--json"], timeoutMs: 5000, sensitive: false };
    case "logs.get": return { executable: NETHOPCTL_PATH, args: ["logs", "get", ...(request.channel ? ["--channel", request.channel] : []), ...listArgs(undefined, request.limit)], timeoutMs: 5000, sensitive: false };
    case "logs.clear": return { executable: NETHOPCTL_PATH, args: ["logs", "clear", "--json"], timeoutMs: 5000, sensitive: false };
    case "connections.get": return { executable: NETHOPCTL_PATH, args: ["connections", ...listArgs(request.query, request.limit)], timeoutMs: 5000, sensitive: false };
    case "connection.close":
      assertSafe(request.connectionId, SAFE_CONNECTION_ID, "connection id");
      return { executable: NETHOPCTL_PATH, args: ["connection", "close", request.connectionId, "--json"], timeoutMs: 5000, sensitive: false };
    case "connections.close-all": return { executable: NETHOPCTL_PATH, args: ["connections", "close-all", "--json"], timeoutMs: 5000, sensitive: false };
    case "diagnostics.bundle": return { executable: NETHOPCTL_PATH, args: ["diagnose", "--json"], timeoutMs: 30000, sensitive: false };
    case "topology.get": return { executable: NETHOPCTL_PATH, args: ["topology", "--json"], timeoutMs: 5000, sensitive: false };
    case "ruleset.status": return { executable: NETHOPCTL_PATH, args: ["ruleset", "status", "--json"], timeoutMs: 5000, sensitive: false };
    case "ruleset.update": return { executable: NETHOPCTL_PATH, args: ["ruleset", "update", "--json", ...(request.wait ? ["--wait"] : [])], timeoutMs: request.wait ? 30000 : 5000, sensitive: false };
    case "core.version-check": return { executable: NETHOPCTL_PATH, args: ["core", "version-check", "--json"], timeoutMs: 10000, sensitive: false };
    case "backup.export": return { executable: NETHOPCTL_PATH, args: ["backup", "export", "--file", "/data/adb/nethop/backups/webui-config-backup.json", "--json"], timeoutMs: 30000, sensitive: true };
    case "webui.payload.create": return { executable: NETHOPCTL_PATH, args: ["webui", "payload", "create", request.namespace, "--json"], timeoutMs: 5000, sensitive: false };
    case "webui.payload.append":
      assertSafe(request.handle, SAFE_HANDLE, "payload handle");
      if (!SAFE_BASE64.test(request.chunk) || request.chunk.length > 16384) throw new Error("invalid payload chunk");
      return { executable: NETHOPCTL_PATH, args: ["webui", "payload", "append", request.namespace, request.handle, request.chunk, "--json"], timeoutMs: 5000, sensitive: true };
    case "webui.payload.commit":
      assertSafe(request.handle, SAFE_HANDLE, "payload handle");
      if (!(payloadOperations[request.namespace] as readonly string[]).includes(request.operation)) throw new Error("invalid payload operation");
      return { executable: NETHOPCTL_PATH, args: ["webui", "payload", "commit", request.namespace, request.handle, request.operation, "--json"], timeoutMs: 30000, sensitive: true };
    case "webui.payload.remove":
      assertSafe(request.handle, SAFE_HANDLE, "payload handle");
      return { executable: NETHOPCTL_PATH, args: ["webui", "payload", "remove", request.namespace, request.handle, "--json"], timeoutMs: 5000, sensitive: true };
  }
}

export function redactCommand(command: BuiltCommand): readonly string[] {
  return command.sensitive ? [command.executable, "[private-payload]"] : [command.executable, ...command.args];
}
