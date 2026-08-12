import { describe, expect, it } from "vitest";

import { safeExtension, ValidationError } from "@/model/bounds";
import { parseApplication, parseApplicationList, parseCapability, parseConfig, parseConfigSchema, parseControlEnvelope, parseEventFrame, parseHello, parseLogs, parseNode, parseNodeDelayList, parseNodeList, parseNodeSelection, parseOperational, parseRuntimeMetrics, parseSourceStatus, parseStatus, parseSubscription, parseSubscriptionSnapshot, parseTraffic } from "@/model/dto";

const digest = "a".repeat(64);

describe("strict DTO validators", () => {
  it("negotiates protocol v3 and denies unknown hello fields", () => {
    const hello = { manager_version: "webui-0.1.0", compatible: true, daemon_protocol_min: 3, daemon_protocol_max: 3, daemon_schema_min: 3, daemon_schema_max: 3, active_schema_version: 3, supported_operations: ["status.get"], supported_features: ["event_stream"] };
    expect(parseHello(hello).compatible).toBe(true);
    expect(() => parseHello({ ...hello, unknown: true })).toThrow("unknown field");
    expect(() => parseHello({ ...hello, manager_version: "x".repeat(65) })).toThrow();
  });

  it("validates control envelope and status state", () => {
    const status = { schema_version: 1, state: "running_tproxy", generation: 2, last_update: "succeeded", watcher_health: "healthy", runtime: {}, subscription: {}, core_update: {}, rule_set: {}, dns_split: {}, capture: {}, operational: {} };
    const envelope = { version: 3, request_id: "test", ok: true, generation: 2, result: status };
    expect(parseControlEnvelope(envelope, parseStatus).result.state).toBe("running_tproxy");
    expect(() => parseStatus({ ...status, state: "invented" })).toThrow("invalid enum");
    expect(() => parseControlEnvelope({ ...envelope, extra: true }, parseStatus)).toThrow("unknown field");
  });

  it("validates capability states and evidence bounds", () => {
    const item = { key: "capture.tproxy.ipv4", status: "supported", reason_code: "probe_supported", requirements: {}, evidence: {}, apply_effect: "network_plan" };
    expect(parseCapability({ schema_version: 1, probe_id: "probe-1", observed_at_monotonic_ms: 10, report_digest: digest, items: [item] }).items[0]?.status).toBe("supported");
    expect(() => parseCapability({ schema_version: 1, probe_id: "probe-1", observed_at_monotonic_ms: 10, report_digest: digest, items: [{ ...item, status: "maybe" }] })).toThrow();
  });

  it("keeps TOML document as the only explicit config extension map", () => {
    const result = parseConfig({ observed_config_digest: digest, active_config_digest: digest, candidate_sequence: 1, watcher_health: "healthy", last_reload: "succeeded", document: { service: { enabled: true } }, source_status: [] });
    expect(result.document).toEqual({ service: { enabled: true } });
    expect(() => parseConfig({ observed_config_digest: "bad", active_config_digest: digest, candidate_sequence: 1, watcher_health: "healthy", last_reload: "succeeded", document: {}, source_status: [] })).toThrow("digest");
  });

  it("parses bounded subscription status without accepting extension fields", () => {
    const status = { source_id: "src_abc", health: "degraded", last_attempt_wall_seconds: 20, last_success_wall_seconds: 10, next_update_wall_seconds: 30, generation: 2, accepted: 56, duplicate: 1, rejected: 2, warnings: 3, subscription_upload_bytes: 128, subscription_download_bytes: 256, subscription_total_bytes: 1024, subscription_expire_at: 2_000_000_000, using_last_known_good: true, diagnostic_code: null };
    expect(parseSourceStatus(status)).toMatchObject({ sourceId: "src_abc", health: "degraded", accepted: 56, subscriptionUploadBytes: 128, subscriptionTotalBytes: 1024, subscriptionExpireAt: 2_000_000_000, usingLastKnownGood: true });
    expect(parseConfig({ observed_config_digest: digest, active_config_digest: digest, candidate_sequence: 1, watcher_health: "healthy", last_reload: "succeeded", document: {}, source_status: [status] }).sourceStatus[0]?.warnings).toBe(3);
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
    const schema = parseConfigSchema({ schema_version: 3, fields: [field], features: [] });

    expect(schema.fields[0]?.options).toEqual(["system", "gvisor"]);
    expect(schema.fields[0]?.capabilityKey).toBe("capture.tun");
    expect(() => parseConfigSchema({ schema_version: 3, fields: [{ ...field, enum: field.enum_values }], features: [] })).toThrow("unknown field");
  });

  it("rejects source URL leakage and credential-shaped node fields", () => {
    expect(parseSubscription({ id: "src_abc", name: "Primary", configured: true, active: true, url_redacted: "[REDACTED]" }).name).toBe("Primary");
    expect(() => parseSubscription({ id: "src_abc", name: "Primary", configured: true, active: true, url_redacted: "https://secret" })).toThrow("redacted");
    const node = { id: "nh1s-0123456789abcdef", name: "Tokyo", protocol: "vless", latency_ms: 23, alive: true, is_requested: true, is_active: true, source_ids: ["src_abc"] };
    expect(parseNode(node).latencyMs).toBe(23);
    expect(() => parseNode({ ...node, password: "secret" })).toThrow("unknown field");
  });

  it("validates application, traffic and all event frame variants", () => {
    expect(parseApplication({ package_name: "tv.danmaku.bili", uid: 10123, mode: "blacklist", shared_uid: false }).uid).toBe(10123);
    expect(parseTraffic({ kind: "traffic", sample: { up: 12, down: 34 }, interval_seconds: 1 }).down).toBe(34);
    expect(() => parseTraffic({ sample: { up: -1, down: Number.NaN }, interval_seconds: 1 })).toThrow();
    const snapshot = { version: 3, request_id: "events", sequence: 1, kind: "item", payload: { kind: "snapshot", runtime: {} } };
    expect(parseEventFrame(snapshot).type).toBe("item");
    expect(parseEventFrame({ version: 3, request_id: "events", sequence: 2, kind: "end" }).type).toBe("end");
    expect(parseEventFrame({ version: 3, request_id: "events", sequence: 3, kind: "error", error: { code: "NH-CORE-UNAVAILABLE", message: "unavailable" } }).type).toBe("error");
    expect(() => parseEventFrame({ ...snapshot, sequence: 0 })).toThrow();
  });

  it("validates channel logs and optional runtime metrics", () => {
    const logs = parseLogs({ entries: [{ seq: 7, kind: "runtime", channel: "core", payload: { kind: "core_started", message: "ready", token: "[REDACTED]" }, raw: "{\"kind\":\"runtime\"}" }], channel: "core", newest_first: true });
    expect(logs[0]).toMatchObject({ id: "7", channel: "core", kind: "core_started", message: "ready" });
    expect(() => parseLogs({ entries: [{ seq: 1, kind: "runtime", channel: "unknown", payload: {}, raw: "{}" }] })).toThrow();
    const metrics = parseRuntimeMetrics({ schema_version: 1, runtime_state: "running_tproxy", generation: 3, uptime_seconds: 90, core: { pid: 12, cpu_percent: 1.5, memory_rss_bytes: 4096 }, traffic: { upload_bytes: 10, download_bytes: 20 }, outbound: { interface: "wlan0", local_address: "192.0.2.2", public_ip: null } });
    expect(metrics).toMatchObject({ runtimeState: "running_tproxy", generation: 3, uptimeSeconds: 90, interface: "wlan0", uploadBytes: 10 });
  });

  it("parses non-empty daemon lists without adding wrapper fields", () => {
    expect(parseSubscriptionSnapshot({ mode: "single", active_source_ids: ["src_abc"], config_digest: digest, sources: [{ id: "src_abc", name: "Primary", configured: true, active: true }] }).sources[0]?.name).toBe("Primary");
    const selection = { version: 1, intent: { mode: "manual", node_id: "nh1s-0123456789abcdef" }, active_node_id: "nh1s-0123456789abcdef", changed_at: 1 };
    expect(parseNodeList({ nodes: [{ id: "nh1s-0123456789abcdef", name: "Tokyo", protocol: "vless", is_requested: true, is_active: true, source_ids: ["src_abc"] }], selection }).selection.intent.mode).toBe("manual");
    expect(() => parseNode({ id: "nh1s-0123456789abcdef", name: "old", protocol: "vless", selected: true, source_ids: ["src_abc"] })).toThrow("unknown field");
    expect(parseNodeSelection(selection).activeNodeId).toBe("nh1s-0123456789abcdef");
    expect(parseApplicationList({ applications: [{ package_name: "tv.danmaku.bili", uid: 10123, mode: "blacklist", shared_uid: false }] })[0]?.packageName).toBe("tv.danmaku.bili");
  });

  it("accepts only bounded stable node IDs in all-node delay results", () => {
    expect(parseNodeDelayList({ results: [{ id: "nh1s-0123456789abcdef", latency_ms: 64 }] })).toEqual([{ id: "nh1s-0123456789abcdef", latencyMs: 64 }]);
    expect(() => parseNodeDelayList({ results: [{ id: "direct", latency_ms: 1 }] })).toThrow("invalid node id");
    expect(() => parseNodeDelayList({ results: [{ id: "nh1s-0123456789abcdef", latency_ms: 65_536 }] })).toThrow();
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
    const selection = { version: 1, intent: { mode: "auto" }, active_node_id: null, changed_at: 0 };
    expect(() => parseNodeList({ nodes: new Array(10_001).fill(node), selection })).toThrow("too many items");
    expect(() => parseNode({ ...node, name: "x".repeat(513) })).toThrow("invalid string");
    expect(() => parseOperational({ logs: new Array(10_001).fill({ message: "bounded" }) }, "logs")).toThrow("too many items");
    expect(() => parseOperational({ connections: [{ address: "x".repeat(16 * 1024 + 1) }] }, "connections")).toThrow("invalid string");
  });
});
