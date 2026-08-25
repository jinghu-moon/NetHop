import { inject, provide, type InjectionKey } from "vue";
import type { HostAdapter } from "./host";
import { detectHostCapability } from "./host";
import { createKernelSuHost } from "./kernelsu-host";
import { createAndroidHost } from "./android-host";
import { createMockHost } from "./mock-host";

const hostKey: InjectionKey<HostAdapter> = Symbol("nethop-host");
const envelope = (result: unknown): string => JSON.stringify({ version: 6, request_id: "mock", ok: true, result });
const mockNowSeconds = Math.floor(Date.now() / 1_000);

export function createAppHost(): HostAdapter {
  const capability = detectHostCapability();
  if (capability.available && capability.kind === "android") return createAndroidHost();
  if (capability.available && capability.kind !== "browser") return createKernelSuHost();
  const primaryId = "src_11111111111111111111111111111111";
  const backupId = "src_22222222222222222222222222222222";
  let mockSubscriptionMode: "single" | "merge" = "single";
  let mockActiveSourceIds = [primaryId];
  let mockConfigRevision = 0;
  let mockManualNodeId: string | undefined;
  const autoNodeId = "nh1s-0123456789abcdef";
  const manualNodeId = "nh1s-fedcba9876543210";
  const originalNodeDocuments: Readonly<Record<string, { readonly displayName: string; readonly outbound: Readonly<Record<string, unknown>> }>> = {
    [autoNodeId]: { displayName: "新加坡 · 高速", outbound: { type: "vless", tag: "proxy", server: "sg.example.com", server_port: 443, uuid: "11111111-1111-4111-8111-111111111111", tls: { enabled: true } } },
    [manualNodeId]: { displayName: "东京 · 低延迟", outbound: { type: "trojan", tag: "proxy", server: "jp.example.com", server_port: 443, password: "preview-secret", tls: { enabled: true } } },
    "nh1s-1111111111111111": { displayName: "洛杉矶 · 备用", outbound: { type: "hysteria2", tag: "proxy", server: "us.example.com", server_port: 443, password: "preview-secret" } },
    "nh1s-2222222222222222": { displayName: "法兰克福", outbound: { type: "vmess", tag: "proxy", server: "de.example.com", server_port: 443, uuid: "22222222-2222-4222-8222-222222222222" } },
  };
  const mockNodeOverrides = new Map<string, { readonly displayName: string; readonly outbound: Readonly<Record<string, unknown>> }>();
  let mockPayloadNamespace: string | undefined;
  let mockPayloadBytes: number[] = [];
  const mockSchemaField = (fieldId: string, valueType: string, group: string, order: number, options: readonly string[] = [], min?: number, max?: number, experimental = false, capabilityKey?: string) => ({
    field_id: fieldId,
    path: fieldId,
    value_type: valueType,
    title_key: `config.${fieldId}.title`,
    description_key: `config.${fieldId}.description`,
    group,
    order,
    advanced: order >= 90,
    experimental,
    sensitive: false,
    read_only: false,
    apply_impact: group === "logging" || fieldId === "subscriptions.auto_update" ? "runtime_only" : group === "network" ? "network_plan" : "generation_activation",
    risk_level: experimental ? "experimental" : "normal",
    ...(options.length > 0 ? { enum_values: options } : {}),
    ...(min === undefined ? {} : { min }),
    ...(max === undefined ? {} : { max }),
    ...(capabilityKey === undefined ? {} : { capability_key: capabilityKey }),
  });
  let mockConfigDocument: Readonly<Record<string, unknown>> = {
    proxy: { outbound_mode: "rule", urltest: { interval_minutes: 10, tolerance_ms: 50, max_candidates: 64 } },
    subscriptions: { auto_update: true, update_interval_hours: 24, mode: "single", sources: [
      { source_id: primaryId, name: "Primary", enabled: true, url_configured: true },
      { source_id: backupId, name: "Backup", enabled: false, url_configured: true },
    ] },
    network: { capture_mode: "auto", proxy_tcp: true, proxy_udp: true, ipv6_mode: "auto", dns_mode: "auto", tun_stack: "gvisor", interfaces: { mobile: true, wifi: true, hotspot: false, usb: false }, wifi_scenes: { enabled: false, probe_interval_seconds: 30 } },
    routing: { bypass_private: true, bypass_cn: false, block_quic: false },
    logging: { level: "info", retention_days: 7 },
    advanced: { inbound_port: 7893, bypass_mark: 131072, ipv6_guard: true, dry_run: false, health_timeout_seconds: 3, reconcile_interval_seconds: 60 },
    applications: { mode: "blacklist", targets: [{ kind: "package", android_user_id: 0, package: "tv.danmaku.bili" }] },
  };
  const nodeDocument = (nodeId: string) => mockNodeOverrides.get(nodeId) ?? originalNodeDocuments[nodeId];
  const mockDigest = (): string => mockConfigRevision === 0 ? "0".repeat(64) : mockConfigRevision.toString(16).padEnd(64, "0");
  const subscriptionSnapshot = () => ({ mode: mockSubscriptionMode, active_source_ids: mockActiveSourceIds, config_digest: mockDigest(), sources: [
    { id: primaryId, name: "Primary", configured: true, active: mockActiveSourceIds.includes(primaryId), node_count: 48, auto_candidate_count: 48 },
    { id: backupId, name: "Backup", configured: true, active: mockActiveSourceIds.includes(backupId), node_count: 24, auto_candidate_count: 24 },
  ] });
  const nodeSelection = () => ({ version: 2, intent: mockManualNodeId ? { mode: "manual", node_id: mockManualNodeId } : { mode: "auto" }, active_terminal: { kind: "node", node_id: mockManualNodeId ?? autoNodeId }, changed_at: mockNowSeconds });
  const nodeSnapshot = () => ({ nodes: [
    { id: autoNodeId, name: nodeDocument(autoNodeId)!.displayName, protocol: nodeDocument(autoNodeId)!.outbound.type, latency_ms: 42, alive: true, is_requested: mockManualNodeId === autoNodeId, is_active: (mockManualNodeId ?? autoNodeId) === autoNodeId, source_ids: [primaryId], display_territory_code: "SG" },
    { id: manualNodeId, name: nodeDocument(manualNodeId)!.displayName, protocol: nodeDocument(manualNodeId)!.outbound.type, latency_ms: null, alive: null, is_requested: mockManualNodeId === manualNodeId, is_active: mockManualNodeId === manualNodeId, source_ids: [primaryId], display_territory_code: "JP" },
    { id: "nh1s-1111111111111111", name: nodeDocument("nh1s-1111111111111111")!.displayName, protocol: nodeDocument("nh1s-1111111111111111")!.outbound.type, latency_ms: 31, alive: true, is_requested: false, is_active: false, source_ids: [backupId], display_territory_code: "US" },
    { id: "nh1s-2222222222222222", name: nodeDocument("nh1s-2222222222222222")!.displayName, protocol: nodeDocument("nh1s-2222222222222222")!.outbound.type, latency_ms: 180, alive: true, is_requested: false, is_active: false, source_ids: [backupId], display_territory_code: "DE" },
  ], selection: nodeSelection() });
  return createMockHost({ responses: {
    hello: { errno: 0, stdout: envelope({ manager_version: "webui-0.1.0", compatible: true, daemon_protocol_min: 6, daemon_protocol_max: 6, daemon_schema_min: 3, daemon_schema_max: 3, active_schema_version: 3, supported_operations: [], supported_features: ["subscription_selection_v3", "node_territory_metadata_v1", "typed_active_terminal_v2", "node_benchmark_fast_selection_v1", "webui_icon_v1"] }), stderr: "" },
    "status.get": { errno: 0, stdout: envelope({ schema_version: 2, state: "fail_open_direct", generation: null, last_update: "never", service: { configured_enabled: true, effective_enabled: true, override: null }, diagnostic_code: "fail_open_direct", watcher_health: {}, runtime: {}, subscription: {}, core_update: {}, rule_set: {}, dns_split: {}, capture: {}, operational: {} }), stderr: "" },
    "traffic.get": { errno: 0, stdout: envelope({ kind: "traffic", state: "ok", sample: { up_bps: 0, down_bps: 0 }, observed_at_unix_ms: mockNowSeconds * 1_000, interval_ms: 1_000 }), stderr: "" },
    "metrics.get": { errno: 0, stdout: envelope({ schema_version: 2, runtime_state: "running_tproxy", generation: 1, uptime_seconds: 3600, core: { pid: 123, cpu_percent: 1.2, memory_rss_bytes: 33554432 }, traffic: { upload_bytes: 1024, download_bytes: 2048 }, outbound: { interface: "wlan0", local_address: "192.0.2.2", public_ip: null } }), stderr: "" },
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
    "config.get": () => ({ errno: 0, stdout: envelope({
      observed_config_digest: mockDigest(), active_config_digest: mockDigest(), candidate_sequence: mockConfigRevision, watcher_health: "healthy", last_reload: mockConfigRevision === 0 ? "never" : "succeeded",
      document: mockConfigDocument,
      source_status: [
        { source_id: "src_11111111111111111111111111111111", health: "healthy", last_attempt_wall_seconds: mockNowSeconds - 3 * 3_600, last_success_wall_seconds: mockNowSeconds - 3 * 3_600, next_update_wall_seconds: mockNowSeconds + 24 * 3_600, generation: 8, accepted: 45, duplicate: 1, rejected: 2, warnings: 1, subscription_upload_bytes: 21_474_836_480, subscription_download_bytes: 116_393_613_722, subscription_total_bytes: 214_748_364_800, subscription_expire_at: mockNowSeconds + 23 * 86_400, using_last_known_good: false, diagnostic_code: null },
        { source_id: "src_22222222222222222222222222222222", health: "never", last_attempt_wall_seconds: null, last_success_wall_seconds: null, next_update_wall_seconds: null, generation: null, accepted: 0, duplicate: 0, rejected: 0, warnings: 0, using_last_known_good: false, diagnostic_code: null },
      ],
    }), stderr: "" }),
    "config.schema": { errno: 0, stdout: envelope({ schema_version: 3, fields: [
      mockSchemaField("subscriptions.auto_update", "boolean", "subscriptions", 20),
      mockSchemaField("subscriptions.update_interval_hours", "integer", "subscriptions", 21, [], 1, 168),
      mockSchemaField("proxy.urltest.interval_minutes", "integer", "proxy", 32, [], 5, 1440),
      mockSchemaField("proxy.urltest.tolerance_ms", "integer", "proxy", 33, [], 0, 1000),
      mockSchemaField("proxy.urltest.max_candidates", "integer", "proxy", 34, [], 1, 64),
      mockSchemaField("network.proxy_tcp", "boolean", "network", 51),
      mockSchemaField("network.proxy_udp", "boolean", "network", 52),
      mockSchemaField("network.ipv6_mode", "enum", "network", 53, ["auto", "proxy", "block"]),
      mockSchemaField("network.dns_mode", "enum", "network", 54, ["auto", "proxy", "system"]),
      mockSchemaField("network.tun_stack", "enum", "network", 55, ["system", "gvisor"], undefined, undefined, false, "capture.tun"),
      mockSchemaField("network.interfaces.mobile", "boolean", "network", 56),
      mockSchemaField("network.interfaces.wifi", "boolean", "network", 57),
      mockSchemaField("network.interfaces.hotspot", "boolean", "network", 58, [], undefined, undefined, true, "capture.hotspot"),
      mockSchemaField("network.interfaces.usb", "boolean", "network", 59, [], undefined, undefined, true, "capture.usb"),
      mockSchemaField("network.wifi_scenes.enabled", "boolean", "network", 62),
      mockSchemaField("network.wifi_scenes.probe_interval_seconds", "integer", "network", 63, [], 15, 3600),
      mockSchemaField("routing.bypass_private", "boolean", "routing", 70),
      mockSchemaField("routing.bypass_cn", "boolean", "routing", 71, [], undefined, undefined, true),
      mockSchemaField("routing.block_quic", "boolean", "routing", 72, [], undefined, undefined, true),
      mockSchemaField("logging.level", "enum", "logging", 80, ["error", "warn", "info", "debug", "trace"]),
      mockSchemaField("logging.retention_days", "integer", "logging", 81, [], 1, 30),
      mockSchemaField("advanced.inbound_port", "integer", "advanced", 90, [], 1, 65535),
      mockSchemaField("advanced.bypass_mark", "integer", "advanced", 91, [], 1, 4294967295),
      mockSchemaField("advanced.ipv6_guard", "boolean", "advanced", 92),
      mockSchemaField("advanced.dry_run", "boolean", "advanced", 93),
      mockSchemaField("advanced.health_timeout_seconds", "integer", "advanced", 94, [], 1, 30),
      mockSchemaField("advanced.reconcile_interval_seconds", "integer", "advanced", 95, [], 60, 3600),
    ], features: [] }), stderr: "" },
    "capability.get": { errno: 0, stdout: envelope({ schema_version: 3, probe_id: "probe-mock", observed_at_monotonic_ms: 0, report_digest: "0".repeat(64), items: [
      { key: "capture.hotspot", status: "unsupported", reason_code: "hotspot_not_available", requirements: {}, evidence: {}, apply_effect: "network_plan" },
      { key: "capture.usb", status: "experimental", reason_code: "usb_capture_experimental", requirements: {}, evidence: {}, apply_effect: "network_plan" },
    ] }), stderr: "" },
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
    "node.override.get": (request) => {
      if (request.id !== "node.override.get") throw new Error("invalid mock operation");
      const document = nodeDocument(request.nodeId);
      if (!document) throw new Error("node is not active");
      return { errno: 0, stdout: envelope({ node_id: request.nodeId, overridden: mockNodeOverrides.has(request.nodeId), display_name: document.displayName, outbound: document.outbound }), stderr: "" };
    },
    "node.override.remove": (request) => {
      if (request.id !== "node.override.remove") throw new Error("invalid mock operation");
      const changed = mockNodeOverrides.delete(request.nodeId);
      return { errno: 0, stdout: envelope({ accepted: true, changed, completed: true }), stderr: "" };
    },
    "node.test-all": () => ({ errno: 0, stdout: envelope({ operation_id: `bench_${"1".repeat(29)}`, phase: "completed", report: { status: "success", trigger: "manual", generation: 1, bootstrap_ms: 1, elapsed_ms: 100, timing: { thread_spawn_us: 100, runtime_init_us: 900, candidate_dispatch_us: 200, probe_us: 98_000, result_assembly_us: 300, total_us: 100_000 }, probe: { first_result_us: 40_000, last_result_us: 70_000, last_success_us: 70_000, completed_within_500ms: 4, completed_within_1s: 4, completed_within_2s: 4, completed_within_3s: 4, completed_before_cutoff: 4, cutoff_pending: 0, cutoff_tail_us: 0 }, tested: 4, succeeded: 4, timed_out: 0, failed: 0, nodes: [
      { node_id: "nh1s-0123456789abcdef", state: "success", latency_ms: 64, request_elapsed_us: 39_000, completed_at_us: 40_000 },
      { node_id: "nh1s-fedcba9876543210", state: "success", latency_ms: 91, request_elapsed_us: 49_000, completed_at_us: 50_000 },
      { node_id: "nh1s-1111111111111111", state: "success", latency_ms: 48, request_elapsed_us: 59_000, completed_at_us: 60_000 },
      { node_id: "nh1s-2222222222222222", state: "success", latency_ms: 188, request_elapsed_us: 69_000, completed_at_us: 70_000 },
    ] }, selection: nodeSelection(), fast_selection: { state: "not_needed", completed: 4, candidate_count: 4, elapsed_us: 100_000 }, timing: { admission_us: 500, worker_reap_us: 200, fast_control: { intent_load_us: 0, current_snapshot_us: 0, decision_us: 0, target_resolve_us: 0, selector_apply_us: 0, final_snapshot_us: 0, total_us: 0 }, terminal_control: { intent_load_us: 50, current_snapshot_us: 0, decision_us: 0, target_resolve_us: 0, selector_apply_us: 0, final_snapshot_us: 250, total_us: 400 }, operation_total_us: 101_100 } }), stderr: "" }),
    "connections.get": { errno: 0, stdout: envelope({ connections: [] }), stderr: "" },
    "logs.get": { errno: 0, stdout: envelope({ entries: [{ seq: 1, kind: "runtime", channel: "service", payload: { kind: "service_ready", message: "daemon ready" }, raw: "{\"kind\":\"runtime\",\"payload\":{\"kind\":\"service_ready\"}}" }], channel: "service", newest_first: true }), stderr: "" },
    "topology.get": { errno: 0, stdout: envelope({ capture_mode: "mock", ipv4: "direct", ipv6: "direct" }), stderr: "" },
    "ruleset.status": { errno: 0, stdout: envelope({ state: "current" }), stderr: "" },
    "core.version-check": { errno: 0, stdout: envelope({ current_version: "1.13.15" }), stderr: "" },
    "webui.payload.create": (request) => {
      if (request.id !== "webui.payload.create") throw new Error("invalid mock operation");
      mockPayloadNamespace = request.namespace;
      mockPayloadBytes = [];
      return { errno: 0, stdout: envelope({ handle: "p_11111111111111111111111111111111" }), stderr: "" };
    },
    "webui.payload.append": (request) => {
      if (request.id !== "webui.payload.append" || request.namespace !== mockPayloadNamespace) throw new Error("invalid mock payload");
      for (const character of atob(request.chunk)) mockPayloadBytes.push(character.charCodeAt(0));
      return { errno: 0, stdout: envelope({ accepted: true }), stderr: "" };
    },
    "webui.payload.commit": (request) => {
      if (request.id !== "webui.payload.commit" || request.namespace !== mockPayloadNamespace) throw new Error("invalid mock payload");
      const payload = JSON.parse(new TextDecoder().decode(Uint8Array.from(mockPayloadBytes))) as { expected_config_digest?: unknown; document?: unknown };
      if (request.namespace === "config") {
        if (request.operation === "config-mutate") {
          mockPayloadNamespace = undefined;
          mockPayloadBytes = [];
          return { errno: 0, stdout: envelope({ accepted: true, changed: true, completed: true, observed_config_digest: mockDigest() }), stderr: "" };
        }
        if (payload.expected_config_digest !== mockDigest() || !payload.document || typeof payload.document !== "object" || Array.isArray(payload.document)) throw new Error("mock config conflict");
        if (request.operation === "config-validate") {
          mockPayloadNamespace = undefined;
          mockPayloadBytes = [];
          return { errno: 0, stdout: envelope({ accepted: true, changed: true, changed_field_ids: ["network.interfaces.wifi"], apply_impact: "network_plan", estimated_disruption: "更新网络接管计划" }), stderr: "" };
        }
        if (request.operation === "config-apply") {
          mockConfigDocument = structuredClone(payload.document as Readonly<Record<string, unknown>>);
          mockConfigRevision += 1;
        }
      }
      if (request.namespace === "node" && request.operation === "node-override-apply") {
        const nodePayload = payload as { target?: unknown; node_override?: { display_name?: unknown; outbound?: unknown } };
        if (typeof nodePayload.target !== "string" || !nodeDocument(nodePayload.target) || typeof nodePayload.node_override?.display_name !== "string" || !nodePayload.node_override.outbound || typeof nodePayload.node_override.outbound !== "object" || Array.isArray(nodePayload.node_override.outbound)) throw new Error("invalid node override payload");
        mockNodeOverrides.set(nodePayload.target, { displayName: nodePayload.node_override.display_name, outbound: nodePayload.node_override.outbound as Readonly<Record<string, unknown>> });
      }
      mockPayloadNamespace = undefined;
      mockPayloadBytes = [];
      return { errno: 0, stdout: envelope({ accepted: true, changed: true, completed: true, observed_config_digest: mockDigest() }), stderr: "" };
    },
    "webui.payload.remove": () => {
      mockPayloadNamespace = undefined;
      mockPayloadBytes = [];
      return { errno: 0, stdout: envelope({ removed: true }), stderr: "" };
    },
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
