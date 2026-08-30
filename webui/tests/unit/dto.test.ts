import { describe, expect, it } from "vitest";

import { safeExtension, ValidationError } from "@/model/bounds";
import { parseApplication, parseApplicationList, parseCapability, parseConfig, parseConfigSchema, parseControlEnvelope, parseEventFrame, parseHello, parseLogs, parseNode, parseNodeBenchmarkAck, parseNodeBenchmarkProgress, parseNodeBenchmarkResult, parseNodeBenchmarkSelection, parseNodeDelayList, parseNodeList, parseNodeSelection, parseOperational, parseRuntimeMetrics, parseSourceStatus, parseStatus, parseSubscription, parseSubscriptionSnapshot, parseTraffic } from "@/model/dto";

const digest = "a".repeat(64);

describe("strict DTO validators", () => {
  it("negotiates protocol v5 and denies unknown hello fields", () => {
    const hello = { manager_version: "webui-0.1.0", compatible: true, daemon_protocol_min: 6, daemon_protocol_max: 6, daemon_schema_min: 3, daemon_schema_max: 3, active_schema_version: 3, supported_operations: ["status.get"], supported_features: ["event_stream", "node_territory_metadata_v1", "typed_active_terminal_v2", "node_benchmark_fast_selection_v1"] };
    expect(parseHello(hello).compatible).toBe(true);
    expect(() => parseHello({ ...hello, unknown: true })).toThrow("unknown field");
    expect(() => parseHello({ ...hello, manager_version: "x".repeat(65) })).toThrow();
  });

  it("validates control envelope and status state", () => {
    const status = { schema_version: 2, state: "running_tproxy", generation: 2, last_update: "succeeded", service: { configured_enabled: true, effective_enabled: true, override: null }, diagnostic_code: null, watcher_health: "healthy", runtime: {}, subscription: {}, core_update: {}, rule_set: {}, dns_split: {}, capture: {}, operational: {} };
    const envelope = { version: 6, request_id: "test", ok: true, generation: 2, result: status };
    expect(parseControlEnvelope(envelope, parseStatus).result).toMatchObject({ state: "running_tproxy", service: { configuredEnabled: true, effectiveEnabled: true } });
    expect(() => parseStatus({ ...status, state: "invented" })).toThrow("invalid enum");
    expect(() => parseStatus({ ...status, schema_version: 1 })).toThrow();
    expect(() => parseStatus({ ...status, service: { ...status.service, override: "ssid:secret" } })).toThrow("invalid enum");
    expect(() => parseStatus({ ...status, diagnostic_code: "free_text" })).toThrow("invalid enum");
    expect(() => parseStatus({ ...status, service: { ...status.service, ssid: "private" } })).toThrow("unknown field");
    expect(() => parseControlEnvelope({ ...envelope, extra: true }, parseStatus)).toThrow("unknown field");
    expect(() => parseControlEnvelope({ ...envelope, version: 3 }, parseStatus)).toThrow("unsupported protocol");
    expect(() => parseControlEnvelope({ ...envelope, version: 5 }, parseStatus)).toThrow("unsupported protocol");
  });

  it("validates capability states and evidence bounds", () => {
    const item = { key: "capture.tproxy.ipv4", status: "supported", reason_code: "probe_supported", requirements: {}, evidence: {}, apply_effect: "network_plan" };
    expect(parseCapability({ schema_version: 1, probe_id: "probe-1", observed_at_monotonic_ms: 10, report_digest: digest, items: [item] }).items[0]?.status).toBe("supported");
    expect(() => parseCapability({ schema_version: 1, probe_id: "probe-1", observed_at_monotonic_ms: 10, report_digest: digest, items: [{ ...item, status: "maybe" }] })).toThrow();
  });

  it("keeps TOML document as the only explicit config extension map", () => {
    const result = parseConfig({ observed_config_digest: digest, active_config_digest: digest, candidate_sequence: 1, watcher_health: "healthy", last_reload: "succeeded", application_runtime: { state: "pending", reason: "unresolved_packages:1" }, document: { service: { enabled: true } }, source_status: [] });
    expect(result.document).toEqual({ service: { enabled: true } });
    expect(result.applicationRuntime).toEqual({ state: "pending", reason: "unresolved_packages:1" });
    expect(() => parseConfig({ observed_config_digest: "bad", active_config_digest: digest, candidate_sequence: 1, watcher_health: "healthy", last_reload: "succeeded", document: {}, source_status: [] })).toThrow("digest");
  });

  it("parses bounded subscription status without accepting extension fields", () => {
    const status = { source_id: "src_abc", health: "degraded", last_attempt_wall_seconds: 20, last_success_wall_seconds: 10, next_update_wall_seconds: 30, generation: 2, accepted: 56, duplicate: 1, rejected: 2, warnings: 3, subscription_upload_bytes: 128, subscription_download_bytes: 256, subscription_total_bytes: 1024, subscription_expire_at: 2_000_000_000, using_last_known_good: true, diagnostic_code: null };
    expect(parseSourceStatus(status)).toMatchObject({ sourceId: "src_abc", health: "degraded", accepted: 56, subscriptionUploadBytes: 128, subscriptionTotalBytes: 1024, subscriptionExpireAt: 2_000_000_000, usingLastKnownGood: true });
    const history = { source_id: "src_abc", attempted_at_wall_seconds: 20, health: "degraded", generation: 2, accepted: 56, duplicate: 1, rejected: 2, warnings: 3, using_last_known_good: true, diagnostic_code: null };
    const config = parseConfig({ observed_config_digest: digest, active_config_digest: digest, candidate_sequence: 1, watcher_health: "healthy", last_reload: "succeeded", document: {}, source_status: [status], source_history: [history] });
    expect(config.sourceStatus[0]?.warnings).toBe(3);
    expect(config.sourceHistory[0]).toMatchObject({ sourceId: "src_abc", health: "degraded", usingLastKnownGood: true });
    expect(() => parseSourceStatus({ ...status, secret_url: "https://secret.invalid" })).toThrow("unknown field");
  });

  it("parses the daemon TUN stack schema with the Android default", () => {
    const field = {
      field_id: "network.tun_stack",
      path: "network.tun_stack",
      value_type: "enum",
      default: "gvisor",
      title_key: "config.network.tun_stack.title",
      description_key: "config.network.tun_stack.description",
      group: "network",
      order: 55,
      advanced: true,
      experimental: false,
      deprecated: false,
      sensitive: false,
      read_only: false,
      write_only: false,
      apply_impact: "generation_activation",
      risk_level: "disruptive",
      capability_key: "capture.tun",
      stage: 2,
      enum_values: ["system", "gvisor"],
    };
    const schema = parseConfigSchema({ schema_version: 3, fields: [{ ...field, range: null, min_items: null }], features: [] });

    expect(schema.fields[0]?.options).toEqual(["system", "gvisor"]);
    expect(schema.fields[0]?.capabilityKey).toBe("capture.tun");
    expect(parseConfigSchema({ schema_version: 3, fields: [{ ...field, enum: field.enum_values }], features: [] }).fields[0]?.options).toEqual(["system", "gvisor"]);
  });

  it("accepts u32 configuration ranges emitted by the daemon", () => {
    const field = {
      field_id: "advanced.bypass_mark", path: "advanced.bypass_mark", value_type: "integer", title_key: "config.advanced.bypass_mark.title",
      description_key: "config.advanced.bypass_mark.description", group: "advanced", order: 91, advanced: true,
      experimental: false, sensitive: false, read_only: false, apply_impact: "network_plan", risk_level: "disruptive",
      enum_values: [], min: 1, max: 4_294_967_295,
    };
    expect(parseConfigSchema({ schema_version: 3, fields: [field], features: [] }).fields[0]?.maximum).toBe(4_294_967_295);
  });

  it("accepts empty and null schema defaults without treating them as user strings", () => {
    const field = {
      field_id: "subscriptions.sources[].url", path: "subscriptions.sources[].url", value_type: "string", default: "",
      title_key: "config.url.title", description_key: "config.url.description", group: "subscriptions", order: 25,
      advanced: false, experimental: false, deprecated: false, sensitive: true, read_only: false, write_only: false,
      apply_impact: "generation_activation", risk_level: "normal", capability_key: null, confirmation_key: null,
      stage: 1, enum: null, enum_values: [], range: null, min: null, max: null, min_items: null, max_items: null,
    };
    expect(parseConfigSchema({ schema_version: 3, fields: [field], features: [] }).fields[0]?.valueType).toBe("string");
  });

  it("rejects source URL leakage and credential-shaped node fields", () => {
    expect(parseSubscription({ id: "src_abc", name: "Primary", configured: true, active: true, url_redacted: "[REDACTED]" }).name).toBe("Primary");
    expect(() => parseSubscription({ id: "src_abc", name: "Primary", configured: true, active: true, url_redacted: "https://secret" })).toThrow("redacted");
    const node = { id: "nh1s-0123456789abcdef", name: "Tokyo", protocol: "vless", latency_ms: 23, alive: true, is_requested: true, is_active: true, source_ids: ["src_abc"], display_territory_code: "JP" };
    expect(parseNode(node)).toMatchObject({ latencyMs: 23, displayTerritoryCode: "JP" });
    expect(() => parseNode({ ...node, display_territory_code: "ZZ" })).toThrow("invalid enum");
    expect(() => parseNode({ ...node, password: "secret" })).toThrow("unknown field");
  });

  it("validates application, traffic and all event frame variants", () => {
    expect(parseApplication({ package_name: "tv.danmaku.bili", uid: 10123, mode: "blacklist", shared_uid: false }).uid).toBe(10123);
    expect(parseTraffic({ kind: "traffic", state: "ok", sample: { up_bps: 12, down_bps: 34 }, observed_at_unix_ms: 1_700_000_000_000, interval_ms: 1_000 }).down).toBe(34);
    expect(parseTraffic({ kind: "traffic", state: "gap", sample: { up_bps: 0, down_bps: 0 }, observed_at_unix_ms: 1_700_000_000_000, interval_ms: 2_000 }).state).toBe("gap");
    expect(() => parseTraffic({ state: "ok", sample: { up_bps: -1, down_bps: Number.NaN }, observed_at_unix_ms: 1, interval_ms: 1_000 })).toThrow();
    const snapshot = { version: 6, request_id: "events", sequence: 1, kind: "item", payload: { kind: "snapshot", runtime: {} } };
    expect(parseEventFrame(snapshot).type).toBe("item");
    expect(parseEventFrame({ version: 6, request_id: "events", sequence: 2, kind: "end" }).type).toBe("end");
    expect(parseEventFrame({ version: 6, request_id: "events", sequence: 3, kind: "error", error: { code: "NH-CORE-UNAVAILABLE", message: "unavailable" } }).type).toBe("error");
    expect(() => parseEventFrame({ ...snapshot, sequence: 0 })).toThrow();
  });

  it("validates channel logs and optional runtime metrics", () => {
    const logs = parseLogs({ entries: [{ seq: 7, kind: "runtime", channel: "core", payload: { kind: "core_started", message: "ready", token: "[REDACTED]" }, raw: "{\"kind\":\"runtime\"}" }], channel: "core", newest_first: true });
    expect(logs[0]).toMatchObject({ id: "7", channel: "core", kind: "core_started", message: "ready" });
    expect(() => parseLogs({ entries: [{ seq: 1, kind: "runtime", channel: "unknown", payload: {}, raw: "{}" }] })).toThrow();
    const metrics = parseRuntimeMetrics({ schema_version: 2, runtime_state: "running_tproxy", generation: 3, uptime_seconds: 90, core: { pid: 12, cpu_percent: 1.5, memory_rss_bytes: 4096 }, traffic: { upload_bytes: 10, download_bytes: 20 }, outbound: { interface: "wlan0", local_address: "192.0.2.2", public_ip: null } });
    expect(metrics).toMatchObject({ runtimeState: "running_tproxy", generation: 3, uptimeSeconds: 90, interface: "wlan0", uploadBytes: 10 });
  });

  it("parses non-empty daemon lists without adding wrapper fields", () => {
    expect(parseSubscriptionSnapshot({ mode: "single", active_source_ids: ["src_abc"], config_digest: digest, sources: [{ id: "src_abc", name: "Primary", configured: true, active: true }] }).sources[0]?.name).toBe("Primary");
    const selection = { version: 2, intent: { mode: "manual", node_id: "nh1s-0123456789abcdef" }, active_terminal: { kind: "node", node_id: "nh1s-0123456789abcdef" }, changed_at: 1 };
    expect(parseNodeList({ nodes: [{ id: "nh1s-0123456789abcdef", name: "Tokyo", protocol: "vless", is_requested: true, is_active: true, source_ids: ["src_abc"] }], selection }).selection.intent.mode).toBe("manual");
    expect(() => parseNode({ id: "nh1s-0123456789abcdef", name: "old", protocol: "vless", selected: true, source_ids: ["src_abc"] })).toThrow("unknown field");
    expect(parseNodeSelection({ version: 2, intent: { mode: "auto" }, active_terminal: { kind: "node", node_id: "nh1s-0123456789abcdef" }, changed_at: 1 }).activeTerminal).toEqual({ kind: "node", nodeId: "nh1s-0123456789abcdef" });
    expect(parseNodeSelection({ version: 2, intent: { mode: "auto" }, active_terminal: { kind: "direct" }, changed_at: 1 }).activeTerminal.kind).toBe("direct");
    expect(parseNodeSelection({ version: 2, intent: { mode: "auto" }, active_terminal: { kind: "block" }, changed_at: 1 }).activeTerminal.kind).toBe("block");
    expect(parseNodeSelection({ version: 2, intent: { mode: "auto" }, active_terminal: { kind: "unresolved", reason: "active_node_unresolved" }, changed_at: 1 }).activeTerminal.kind).toBe("unresolved");
    expect(parseApplicationList({ applications: [{ package_name: "tv.danmaku.bili", uid: 10123, mode: "blacklist", shared_uid: false }] })[0]?.packageName).toBe("tv.danmaku.bili");
  });

  it("accepts only bounded stable node IDs in all-node delay results", () => {
    expect(parseNodeDelayList({ results: [{ id: "nh1s-0123456789abcdef", latency_ms: 64 }] })).toEqual([{ id: "nh1s-0123456789abcdef", latencyMs: 64 }]);
    expect(() => parseNodeDelayList({ results: [{ id: "direct", latency_ms: 1 }] })).toThrow("invalid node id");
    expect(() => parseNodeDelayList({ results: [{ id: "nh1s-0123456789abcdef", latency_ms: 65_536 }] })).toThrow();
  });

  it("validates benchmark ACK and terminal report invariants", () => {
    const operationId = `bench_${"1".repeat(29)}`;
    const ack = { operation_id: operationId, phase: "running", joined_existing: true, trigger: "periodic", candidate_count: 64, fast_selection_earliest_ms: 2000, fast_selection_latest_ms: 2800, fast_selection_deadline_ms: 3000, probe_cutoff_ms: 4500, deadline_ms: 4900, fast_selection: { state: "pending" } };
    expect(parseNodeBenchmarkAck(ack)).toMatchObject({ joinedExisting: true, fastSelection: { state: "pending" }, fastSelectionDeadlineMs: 3000 });
    expect(() => parseNodeBenchmarkAck({ ...ack, candidate_count: 65 })).toThrow();
    const terminal = {
      operation_id: operationId,
      phase: "completed",
      report: {
        status: "partial", trigger: "manual", generation: 7, bootstrap_ms: 1, elapsed_ms: 100,
        timing: { thread_spawn_us: 100, runtime_init_us: 900, candidate_dispatch_us: 200, probe_us: 98_000, result_assembly_us: 300, total_us: 100_000 },
        probe: { first_result_us: 90_000, last_result_us: 90_000, last_success_us: 90_000, completed_within_500ms: 1, completed_within_1s: 1, completed_within_2s: 1, completed_within_3s: 1, completed_before_cutoff: 1, cutoff_pending: 1, cutoff_tail_us: 10_000 },
        tested: 2, succeeded: 1, timed_out: 1, failed: 0, diagnostic: "unauthorized",
        nodes: [{ node_id: "nh1s-0123456789abcdef", state: "success", latency_ms: 42, request_elapsed_us: 80_000, completed_at_us: 90_000 }, { node_id: "nh1s-fedcba9876543210", state: "timeout", request_elapsed_us: 98_000, completed_at_us: 100_000 }],
      },
      fast_selection: { state: "not_needed", completed: 2, candidate_count: 2, elapsed_us: 100_000 },
      timing: {
        admission_us: 500,
        worker_reap_us: 200,
        fast_control: { intent_load_us: 0, current_snapshot_us: 0, decision_us: 0, target_resolve_us: 0, selector_apply_us: 0, final_snapshot_us: 0, total_us: 0 },
        terminal_control: { intent_load_us: 50, current_snapshot_us: 0, decision_us: 0, target_resolve_us: 0, selector_apply_us: 0, final_snapshot_us: 250, total_us: 400 },
        operation_total_us: 101_100,
      },
    };
    const parsed = parseNodeBenchmarkResult(terminal);
    expect(parsed.report.diagnostic).toBe("unauthorized");
    expect(parsed.report.timing.probeUs).toBe(98_000);
    expect(parsed.report.probe).toMatchObject({ completedWithin500ms: 1, completedBeforeCutoff: 1, cutoffPending: 1, cutoffTailUs: 10_000 });
    expect(parsed.report.nodes[0]).toMatchObject({ requestElapsedUs: 80_000, completedAtUs: 90_000 });
    expect(parseNodeBenchmarkResult(terminal).timing.terminalControl.finalSnapshotUs).toBe(250);
    expect(parseNodeBenchmarkSelection({ operation_id: operationId, phase: "selection", generation: 7, fast_selection: { state: "kept", completed: 43, candidate_count: 64, elapsed_us: 2_000_000 } }).fastSelection.state).toBe("kept");
    expect(() => parseNodeBenchmarkSelection({ operation_id: operationId, phase: "selection", generation: 7, fast_selection: { state: "pending" } })).toThrow("selection milestone is pending");
    expect(() => parseNodeBenchmarkSelection({ operation_id: operationId, phase: "selection", generation: 7, fast_selection: { state: "switched", completed: 43, candidate_count: 64, elapsed_us: 3_000_001, selection: { version: 2, intent: { mode: "auto" }, active_terminal: { kind: "node", node_id: "nh1s-0123456789abcdef" }, changed_at: 2 } } })).toThrow();
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, succeeded: 2 } })).toThrow("inconsistent benchmark counts");
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, elapsed_ms: 4901 } })).toThrow();
    expect(() => parseNodeBenchmarkResult({ ...terminal, timing: { ...terminal.timing, operation_total_us: 100 } })).toThrow();
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, nodes: [{ node_id: "nh1s-0123456789abcdef", state: "success" }], tested: 1, succeeded: 1, timed_out: 0 } })).toThrow("invalid probe outcome");
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, probe: undefined } })).toThrow();
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, probe: { ...terminal.report.probe, completed_within_1s: 0 } } })).toThrow("inconsistent probe summary");
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, nodes: [{ ...terminal.report.nodes[0], request_elapsed_us: 91_000 }, terminal.report.nodes[1]] } })).toThrow("inconsistent probe timing");
    expect(() => parseNodeBenchmarkResult({ ...terminal, report: { ...terminal.report, internal_tag: "private" } })).toThrow("unknown field");
    expect(parseNodeBenchmarkProgress({ operation_id: operationId, phase: "progress", generation: 7, completed: 1, candidate_count: 2, outcome: { node_id: "nh1s-0123456789abcdef", state: "success", latency_ms: 42, request_elapsed_us: 40_000, completed_at_us: 41_000 } }).outcome.latencyMs).toBe(42);
    expect(() => parseNodeBenchmarkProgress({ operation_id: operationId, phase: "progress", generation: 7, completed: 3, candidate_count: 2, outcome: { node_id: "nh1s-0123456789abcdef", state: "success", latency_ms: 42, request_elapsed_us: 40_000, completed_at_us: 41_000 } })).toThrow();
  });

  it("bounds operational extensions and 10,000-node corpus", () => {
    expect(parseOperational({ connections: [] }, "connections")).toEqual({ connections: [] });
    const nodes = Array.from({ length: 10_000 }, (_, index) => parseNode({ id: `nh1s-${index.toString(16).padStart(16, "0")}`, name: `Node ${index}`, protocol: "trojan", is_requested: false, is_active: false, source_ids: ["src_abc"] }));
    expect(nodes).toHaveLength(10_000);
    expect(nodes.every((node) => !("password" in node))).toBe(true);
  });
});

describe("bounded corpus", () => {
  it("rejects depth, huge arrays and prototype-shaped keys as typed failures", () => {
    let deep: unknown = "leaf";
    for (let index = 0; index < 40; index += 1) deep = { next: deep };
    const cases: unknown[] = [deep, new Array(10_001).fill(0), JSON.parse('{"__proto__":{"polluted":true}}')];
    for (const value of cases) expect(() => safeExtension(value)).toThrow(ValidationError);
    expect(({} as { polluted?: boolean }).polluted).toBeUndefined();
  });

  it("rejects oversized external collections and strings before store admission", () => {
    const node = { id: "nh1s-0123456789abcdef", name: "Node", protocol: "trojan", is_requested: false, is_active: false, source_ids: ["src_abc"] };
    const selection = { version: 2, intent: { mode: "auto" }, active_terminal: { kind: "unresolved", reason: "active_node_unresolved" }, changed_at: 0 };
    expect(() => parseNodeList({ nodes: new Array(10_001).fill(node), selection })).toThrow("too many items");
    expect(() => parseNode({ ...node, name: "x".repeat(513) })).toThrow("invalid string");
    expect(() => parseOperational({ logs: new Array(10_001).fill({ message: "bounded" }) }, "logs")).toThrow("too many items");
    expect(() => parseOperational({ connections: [{ address: "x".repeat(16 * 1024 + 1) }] }, "connections")).toThrow("invalid string");
  });
});
