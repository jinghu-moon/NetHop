import { inject, provide, type InjectionKey } from "vue";
import type { HostAdapter } from "./host";
import { detectHostCapability } from "./host";
import { createKernelSuHost } from "./kernelsu-host";
import { createMockHost } from "./mock-host";

const hostKey: InjectionKey<HostAdapter> = Symbol("nethop-host");
const envelope = (result: unknown): string => JSON.stringify({ version: 3, request_id: "mock", ok: true, result });
const mockNowSeconds = Math.floor(Date.now() / 1_000);

export function createAppHost(): HostAdapter {
  const capability = detectHostCapability();
  if (capability.available && capability.kind !== "browser") return createKernelSuHost();
  const primaryId = "src_11111111111111111111111111111111";
  const backupId = "src_22222222222222222222222222222222";
  let mockSubscriptionMode: "single" | "merge" = "single";
  let mockActiveSourceIds = [primaryId];
  let mockConfigRevision = 0;
  let mockManualNodeId: string | undefined;
  const autoNodeId = "nh1s-0123456789abcdef";
  const manualNodeId = "nh1s-fedcba9876543210";
  const mockDigest = (): string => mockConfigRevision.toString(16).padStart(64, "0");
  const subscriptionSnapshot = () => ({ mode: mockSubscriptionMode, active_source_ids: mockActiveSourceIds, config_digest: mockDigest(), sources: [
    { id: primaryId, name: "Primary", configured: true, active: mockActiveSourceIds.includes(primaryId), node_count: 48, auto_candidate_count: 48 },
    { id: backupId, name: "Backup", configured: true, active: mockActiveSourceIds.includes(backupId), node_count: 24, auto_candidate_count: 24 },
  ] });
  const nodeSelection = () => ({ version: 1, intent: mockManualNodeId ? { mode: "manual", node_id: mockManualNodeId } : { mode: "auto" }, active_node_id: mockManualNodeId ?? autoNodeId, changed_at: mockNowSeconds });
  const nodeSnapshot = () => ({ nodes: [
    { id: autoNodeId, name: "新加坡 · 高速", protocol: "vless", latency_ms: 42, alive: true, is_requested: mockManualNodeId === autoNodeId, is_active: (mockManualNodeId ?? autoNodeId) === autoNodeId, source_ids: [primaryId] },
    { id: manualNodeId, name: "东京 · 低延迟", protocol: "trojan", latency_ms: null, alive: null, is_requested: mockManualNodeId === manualNodeId, is_active: mockManualNodeId === manualNodeId, source_ids: [primaryId] },
    { id: "nh1s-1111111111111111", name: "洛杉矶 · 备用", protocol: "hysteria2", latency_ms: 31, alive: true, is_requested: false, is_active: false, source_ids: [backupId] },
    { id: "nh1s-2222222222222222", name: "法兰克福", protocol: "vmess", latency_ms: 180, alive: true, is_requested: false, is_active: false, source_ids: [backupId] },
  ], selection: nodeSelection() });
  return createMockHost({ responses: {
    hello: { errno: 0, stdout: envelope({ manager_version: "webui-0.1.0", compatible: true, daemon_protocol_min: 3, daemon_protocol_max: 3, daemon_schema_min: 3, daemon_schema_max: 3, active_schema_version: 3, supported_operations: [], supported_features: ["subscription_selection_v3"] }), stderr: "" },
    "status.get": { errno: 0, stdout: envelope({ schema_version: 3, state: "fail_open_direct", generation: null, last_update: "never", watcher_health: {}, runtime: {}, subscription: {}, core_update: {}, rule_set: {}, dns_split: {}, capture: {}, operational: {} }), stderr: "" },
    "traffic.get": { errno: 0, stdout: envelope({ kind: "traffic", sample: { up: 0, down: 0 }, interval_seconds: 1 }), stderr: "" },
    "metrics.get": { errno: 0, stdout: envelope({ schema_version: 1, runtime_state: "running_tproxy", generation: 1, uptime_seconds: 3600, core: { pid: 123, cpu_percent: 1.2, memory_rss_bytes: 33554432 }, traffic: { upload_bytes: 1024, download_bytes: 2048 }, outbound: { interface: "wlan0", local_address: "192.0.2.2", public_ip: null } }), stderr: "" },
    "subscription.list": { errno: 0, stdout: envelope({ subscriptions: [
      { id: "src_11111111111111111111111111111111", name: "Primary", configured: true, active: true, url_redacted: "[REDACTED]" },
      { id: "src_22222222222222222222222222222222", name: "Backup", configured: true, active: false, url_redacted: "[REDACTED]" },
    ] }), stderr: "" },
    "subscription.mode.get": () => ({ errno: 0, stdout: envelope(subscriptionSnapshot()), stderr: "" }),
    "subscription.mode.set": (request) => {
      if (request.id !== "subscription.mode.set") throw new Error("invalid mock operation");
      mockSubscriptionMode = request.mode;
      if (request.mode === "single") mockActiveSourceIds = [request.sourceId!];
      mockConfigRevision += 1;
      return { errno: 0, stdout: envelope({ accepted: true, changed: true, completed: true, config_digest: mockDigest() }), stderr: "" };
    },
    "subscription.select": (request) => {
      if (request.id !== "subscription.select") throw new Error("invalid mock operation");
      mockActiveSourceIds = [request.sourceId];
      mockConfigRevision += 1;
      return { errno: 0, stdout: envelope({ accepted: true, changed: true, completed: true, config_digest: mockDigest() }), stderr: "" };
    },
    "subscription.set-enabled": (request) => {
      if (request.id !== "subscription.set-enabled") throw new Error("invalid mock operation");
      mockActiveSourceIds = request.enabled
        ? [...new Set([...mockActiveSourceIds, request.sourceId])]
        : mockActiveSourceIds.filter((id) => id !== request.sourceId);
      mockConfigRevision += 1;
      return { errno: 0, stdout: envelope({ accepted: true, changed: true, completed: true, config_digest: mockDigest() }), stderr: "" };
    },
    "config.get": { errno: 0, stdout: envelope({
      observed_config_digest: "0".repeat(64), active_config_digest: "0".repeat(64), candidate_sequence: 0, watcher_health: "healthy", last_reload: "never",
      document: {
        proxy: { outbound_mode: "rule" }, network: { capture_mode: "auto" },
        applications: { mode: "blacklist", targets: [{ kind: "package", android_user_id: 0, package: "tv.danmaku.bili" }] },
        subscriptions: { mode: "single", sources: [
          { source_id: "src_11111111111111111111111111111111", name: "Primary", enabled: true, url_configured: true },
          { source_id: "src_22222222222222222222222222222222", name: "Backup", enabled: false, url_configured: true },
        ] },
      },
      source_status: [
        { source_id: "src_11111111111111111111111111111111", health: "healthy", last_attempt_wall_seconds: mockNowSeconds - 3 * 3_600, last_success_wall_seconds: mockNowSeconds - 3 * 3_600, next_update_wall_seconds: mockNowSeconds + 24 * 3_600, generation: 8, accepted: 45, duplicate: 1, rejected: 2, warnings: 1, subscription_upload_bytes: 21_474_836_480, subscription_download_bytes: 116_393_613_722, subscription_total_bytes: 214_748_364_800, subscription_expire_at: mockNowSeconds + 23 * 86_400, using_last_known_good: false, diagnostic_code: null },
        { source_id: "src_22222222222222222222222222222222", health: "never", last_attempt_wall_seconds: null, last_success_wall_seconds: null, next_update_wall_seconds: null, generation: null, accepted: 0, duplicate: 0, rejected: 0, warnings: 0, using_last_known_good: false, diagnostic_code: null },
      ],
    }), stderr: "" },
    "config.schema": { errno: 0, stdout: envelope({ schema_version: 3, fields: [], features: [] }), stderr: "" },
    "capability.get": { errno: 0, stdout: envelope({ schema_version: 3, probe_id: "probe-mock", observed_at_monotonic_ms: 0, report_digest: "0".repeat(64), items: [] }), stderr: "" },
    "node.list": () => ({ errno: 0, stdout: envelope(nodeSnapshot()), stderr: "" }),
    "node.selection.get": () => ({ errno: 0, stdout: envelope(nodeSelection()), stderr: "" }),
    "node.select.auto": () => {
      mockManualNodeId = undefined;
      return { errno: 0, stdout: envelope(nodeSelection()), stderr: "" };
    },
    "node.select.manual": (request) => {
      if (request.id !== "node.select.manual") throw new Error("invalid mock operation");
      mockManualNodeId = request.nodeId;
      return { errno: 0, stdout: envelope(nodeSelection()), stderr: "" };
    },
    "node.test-all": () => ({ errno: 0, stdout: envelope({ operation_id: `bench_${"1".repeat(29)}`, phase: "completed", report: { status: "success", trigger: "manual", generation: 1, bootstrap_ms: 1, elapsed_ms: 100, tested: 4, succeeded: 4, timed_out: 0, failed: 0, nodes: [
      { node_id: "nh1s-0123456789abcdef", state: "success", latency_ms: 64 },
      { node_id: "nh1s-fedcba9876543210", state: "success", latency_ms: 91 },
      { node_id: "nh1s-1111111111111111", state: "success", latency_ms: 48 },
      { node_id: "nh1s-2222222222222222", state: "success", latency_ms: 188 },
    ] }, selection: nodeSelection() }), stderr: "" }),
    "connections.get": { errno: 0, stdout: envelope({ connections: [] }), stderr: "" },
    "logs.get": { errno: 0, stdout: envelope({ entries: [{ seq: 1, kind: "runtime", channel: "service", payload: { kind: "service_ready", message: "daemon ready" }, raw: "{\"kind\":\"runtime\",\"payload\":{\"kind\":\"service_ready\"}}" }], channel: "service", newest_first: true }), stderr: "" },
    "topology.get": { errno: 0, stdout: envelope({ capture_mode: "mock", ipv4: "direct", ipv6: "direct" }), stderr: "" },
    "ruleset.status": { errno: 0, stdout: envelope({ state: "current" }), stderr: "" },
    "core.version-check": { errno: 0, stdout: envelope({ current_version: "1.13.15" }), stderr: "" },
    "webui.payload.create": { errno: 0, stdout: envelope({ handle: "p_11111111111111111111111111111111" }), stderr: "" },
    "webui.payload.append": { errno: 0, stdout: envelope({ accepted: true }), stderr: "" },
    "webui.payload.commit": { errno: 0, stdout: envelope({ accepted: true, changed: true, completed: true, observed_config_digest: "1".repeat(64) }), stderr: "" },
    "webui.payload.remove": { errno: 0, stdout: envelope({ removed: true }), stderr: "" },
  }, packages: [
    { packageName: "tv.danmaku.bili", versionName: "8.68.0", versionCode: 8680000, appLabel: "哔哩哔哩", isSystem: false, uid: 10123, lastUpdateTimeMs: 2_000_000_000_000, storageBytes: 640_000_000, lastUsedTimeMs: 2_000_000_300_000 },
    { packageName: "com.google.android.youtube", versionName: "21.30", versionCode: 2130000, appLabel: "YouTube", isSystem: false, uid: 10124, lastUpdateTimeMs: 2_000_000_100_000, storageBytes: 920_000_000, lastUsedTimeMs: 2_000_000_200_000 },
    { packageName: "com.android.settings", versionName: "16", versionCode: 36, appLabel: "设置", isSystem: true, uid: 1000, lastUpdateTimeMs: 2_000_000_200_000, storageBytes: 120_000_000, lastUsedTimeMs: 2_000_000_100_000 },
  ] });
}

export function provideHost(host: HostAdapter): void { provide(hostKey, host); }
export function useHost(): HostAdapter {
  const host = inject(hostKey);
  if (!host) throw new Error("HostAdapter is not available");
  return host;
}
