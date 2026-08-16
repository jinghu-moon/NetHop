import { array, boolean, digest, enumeration, finiteNumber, integer, optionalString, record, safeExtension, string, ValidationError } from "./bounds";
import { territoryCodes, type TerritoryCode } from "@/generated/territories";

export const RUNTIME_STATES = ["init", "probing", "starting_core", "running_tproxy", "starting_tun", "running_tun", "degraded", "fail_open_direct", "backoff", "circuit_open", "stopping"] as const;
export const EVENT_KINDS = ["snapshot", "config", "runtime", "subscription", "generation", "network", "traffic", "subscription_mode", "subscription_active_set", "node_selection", "node_active", "node_test", "resync_required", "operation"] as const;

export interface ControlEnvelope<T> { readonly version: 5; readonly requestId: string; readonly generation?: number; readonly result: T }
export interface HelloDto { readonly compatible: boolean; readonly daemonProtocolMin: number; readonly daemonProtocolMax: number; readonly supportedOperations: readonly string[]; readonly supportedFeatures: readonly string[] }
export type ServiceOverrideDto = "wifi_scene";
export type StatusDiagnosticCodeDto = "config_unavailable" | "fail_open_direct" | "runtime_degraded" | "runtime_backoff" | "runtime_circuit_open";
export interface ServiceStatusDto { readonly configuredEnabled: boolean; readonly effectiveEnabled: boolean; readonly override?: ServiceOverrideDto }
export interface StatusDto { readonly schemaVersion: 2; readonly state: typeof RUNTIME_STATES[number]; readonly generation?: number; readonly lastUpdate: "never" | "succeeded" | "failed"; readonly service: ServiceStatusDto; readonly diagnosticCode?: StatusDiagnosticCodeDto; readonly extension: Readonly<Record<string, unknown>> }
export interface CapabilityItemDto { readonly key: string; readonly status: "supported" | "unsupported" | "experimental" | "conflict" | "unavailable"; readonly reasonCode: string; readonly applyEffect: string }
export interface CapabilityDto { readonly schemaVersion: number; readonly probeId: string; readonly observedAtMonotonicMs: number; readonly reportDigest: string; readonly items: readonly CapabilityItemDto[] }
export type SubscriptionHealth = "never" | "healthy" | "degraded" | "failed";
export interface SourceStatusDto {
  readonly sourceId: string;
  readonly health: SubscriptionHealth;
  readonly lastAttemptWallSeconds?: number;
  readonly lastSuccessWallSeconds?: number;
  readonly nextUpdateWallSeconds?: number;
  readonly generation?: number;
  readonly accepted: number;
  readonly duplicate: number;
  readonly rejected: number;
  readonly warnings: number;
  readonly subscriptionUploadBytes?: number;
  readonly subscriptionDownloadBytes?: number;
  readonly subscriptionTotalBytes?: number;
  readonly subscriptionExpireAt?: number;
  readonly usingLastKnownGood: boolean;
  readonly diagnosticCode?: string;
}
export interface ConfigDto { readonly observedConfigDigest: string; readonly activeConfigDigest: string; readonly candidateSequence: number; readonly document: Readonly<Record<string, unknown>>; readonly sourceStatus: readonly SourceStatusDto[] }
export type SubscriptionModeDto = "single" | "merge";
export interface SubscriptionDto { readonly id: string; readonly name: string; readonly configured: boolean; readonly active: boolean; readonly nodeCount?: number; readonly autoCandidateCount?: number; readonly state?: string; readonly status?: SourceStatusDto }
export interface SubscriptionSnapshotDto { readonly mode: SubscriptionModeDto; readonly activeSourceIds: readonly string[]; readonly sources: readonly SubscriptionDto[]; readonly configDigest: string }
export type NodeSelectionIntentDto = { readonly mode: "auto" } | { readonly mode: "manual"; readonly nodeId: string };
export type ActiveTerminalDto =
  | { readonly kind: "node"; readonly nodeId: string }
  | { readonly kind: "direct" }
  | { readonly kind: "block" }
  | { readonly kind: "unresolved"; readonly reason: string };
export interface NodeSelectionDto { readonly version: 2; readonly intent: NodeSelectionIntentDto; readonly activeTerminal: ActiveTerminalDto; readonly changedAt: number }
export interface NodeDto { readonly id: string; readonly name: string; readonly protocol: string; readonly latencyMs?: number; readonly alive?: boolean; readonly isRequested: boolean; readonly isActive: boolean; readonly sourceIds: readonly string[]; readonly displayTerritoryCode?: TerritoryCode }
export interface NodeListSnapshotDto { readonly nodes: readonly NodeDto[]; readonly selection: NodeSelectionDto }
export type LogChannelDto = "service" | "subscription" | "core";
export interface LogEntryDto { readonly id: string; readonly channel: LogChannelDto; readonly kind: string; readonly message: string; readonly time: string; readonly raw: string }
export interface RuntimeMetricsDto { readonly runtimeState: string; readonly generation?: number; readonly uptimeSeconds: number; readonly core?: { readonly pid: number; readonly cpuPercent?: number; readonly memoryRssBytes?: number }; readonly uploadBytes?: number; readonly downloadBytes?: number; readonly interface?: string; readonly localAddress?: string; readonly publicIp?: string }
export interface NodeDelayDto { readonly id: string; readonly latencyMs: number }
export type NodeProbeStateDto = "success" | "timeout" | "unavailable" | "protocol_error";
export interface NodeProbeOutcomeDto { readonly id: string; readonly state: NodeProbeStateDto; readonly latencyMs?: number; readonly requestElapsedUs: number; readonly completedAtUs: number }
export interface NodeBenchmarkProgressDto { readonly operationId: string; readonly phase: "progress"; readonly generation: number; readonly completed: number; readonly candidateCount: number; readonly outcome: NodeProbeOutcomeDto }
export interface BenchmarkEngineTimingDto { readonly threadSpawnUs: number; readonly runtimeInitUs: number; readonly candidateDispatchUs: number; readonly probeUs: number; readonly resultAssemblyUs: number; readonly totalUs: number }
export interface BenchmarkProbeSummaryDto { readonly firstResultUs?: number; readonly lastResultUs?: number; readonly lastSuccessUs?: number; readonly completedWithin500ms: number; readonly completedWithin1s: number; readonly completedWithin2s: number; readonly completedWithin3s: number; readonly completedBeforeCutoff: number; readonly cutoffPending: number; readonly cutoffTailUs: number }
export interface BenchmarkControlTimingDto { readonly intentLoadUs: number; readonly currentSnapshotUs: number; readonly decisionUs: number; readonly targetResolveUs: number; readonly selectorApplyUs: number; readonly finalSnapshotUs: number; readonly totalUs: number }
export type FastSelectionDeferredReasonDto = "insufficient_coverage" | "current_pending" | "no_success" | "control_error";
type FastSelectionMetricsDto = { readonly completed: number; readonly candidateCount: number; readonly elapsedUs: number };
export type NodeBenchmarkFastSelectionDto =
  | { readonly state: "pending" }
  | ({ readonly state: "switched"; readonly selection: NodeSelectionDto } & FastSelectionMetricsDto)
  | ({ readonly state: "kept" | "not_applicable" | "superseded" | "not_needed" } & FastSelectionMetricsDto)
  | ({ readonly state: "deferred"; readonly reason: FastSelectionDeferredReasonDto } & FastSelectionMetricsDto);
export interface NodeBenchmarkSelectionDto { readonly operationId: string; readonly phase: "selection"; readonly generation: number; readonly fastSelection: NodeBenchmarkFastSelectionDto }
export interface BenchmarkTerminalTimingDto { readonly admissionUs: number; readonly workerReapUs: number; readonly fastControl: BenchmarkControlTimingDto; readonly terminalControl: BenchmarkControlTimingDto; readonly operationTotalUs: number }
export interface NodeBenchmarkReportDto { readonly status: "success" | "partial" | "failed" | "internal_error" | "superseded"; readonly trigger: "manual" | "periodic"; readonly generation: number; readonly bootstrapMs: number; readonly elapsedMs: number; readonly timing: BenchmarkEngineTimingDto; readonly probe: BenchmarkProbeSummaryDto; readonly tested: number; readonly succeeded: number; readonly timedOut: number; readonly failed: number; readonly diagnostic?: "unauthorized"; readonly nodes: readonly NodeProbeOutcomeDto[] }
export interface NodeBenchmarkAckDto { readonly operationId: string; readonly phase: "running"; readonly joinedExisting: boolean; readonly trigger: "manual" | "periodic"; readonly candidateCount: number; readonly fastSelectionEarliestMs: 2000; readonly fastSelectionLatestMs: 2800; readonly fastSelectionDeadlineMs: 3000; readonly probeCutoffMs: 4500; readonly deadlineMs: 4900; readonly fastSelection: NodeBenchmarkFastSelectionDto }
export interface NodeBenchmarkResultDto { readonly operationId: string; readonly phase: "completed"; readonly report: NodeBenchmarkReportDto; readonly selection?: NodeSelectionDto; readonly fastSelection: Exclude<NodeBenchmarkFastSelectionDto, { readonly state: "pending" }>; readonly timing: BenchmarkTerminalTimingDto }
export interface ApplicationDto { readonly packageName: string; readonly uid: number; readonly mode: "all" | "blacklist" | "whitelist"; readonly sharedUid: boolean }
export interface TrafficDto { readonly up: number; readonly down: number; readonly intervalSeconds: number }
export interface ConfigSchemaFieldDto { readonly id: string; readonly path: string; readonly valueType: string; readonly title: string; readonly group: string; readonly order: number; readonly advanced: boolean; readonly experimental: boolean; readonly sensitive: boolean; readonly readOnly: boolean; readonly applyImpact: string; readonly riskLevel: string; readonly capabilityKey?: string; readonly options: readonly string[] }
export interface ConfigSchemaDto { readonly schemaVersion: number; readonly fields: readonly ConfigSchemaFieldDto[] }

export type EventFrameDto =
  | { readonly type: "item"; readonly requestId: string; readonly sequence: number; readonly payload: EventPayloadDto }
  | { readonly type: "end"; readonly requestId: string; readonly sequence: number }
  | { readonly type: "error"; readonly requestId: string; readonly sequence: number; readonly error: ErrorDto };
export interface EventPayloadDto { readonly kind: typeof EVENT_KINDS[number]; readonly value: Readonly<Record<string, unknown>>; readonly traffic?: TrafficDto }
export interface ErrorDto { readonly code: string; readonly message: string; readonly details?: unknown }

export function parseControlEnvelope<T>(value: unknown, parseResult: (value: unknown) => T): ControlEnvelope<T> {
  const object = record(value, "$", ["version", "request_id", "ok", "generation", "result", "error"]);
  if (integer(object.version, "$.version", 1, 255) !== 5) throw new ValidationError("$.version", "unsupported protocol");
  const requestId = string(object.request_id, "$.request_id", 96);
  const ok = boolean(object.ok, "$.ok");
  const generation = object.generation === undefined || object.generation === null ? undefined : integer(object.generation, "$.generation", 1);
  if (!ok) throw parseError(object.error);
  if (object.error !== undefined) throw new ValidationError("$.error", "unexpected error");
  return { version: 5, requestId, ...(generation === undefined ? {} : { generation }), result: parseResult(object.result) };
}

export function parseError(value: unknown): ValidationError {
  const object = record(value, "$.error", ["code", "message", "details"]);
  const code = string(object.code, "$.error.code", 96);
  const message = string(object.message, "$.error.message", 1024);
  if (!/^NH-[A-Z]+-[A-Z0-9-]+$/.test(code)) return new ValidationError("$.error.code", "invalid stable error code");
  if (object.details !== undefined) safeExtension(object.details, "$.error.details");
  return new ValidationError(code, message);
}

export function parseHello(value: unknown): HelloDto {
  const object = record(value, "$.result", ["manager_version", "compatible", "daemon_protocol_min", "daemon_protocol_max", "daemon_schema_min", "daemon_schema_max", "active_schema_version", "supported_operations", "supported_features"]);
  string(object.manager_version, "$.result.manager_version", 64);
  integer(object.daemon_schema_min, "$.result.daemon_schema_min", 1, 255);
  integer(object.daemon_schema_max, "$.result.daemon_schema_max", 1, 255);
  integer(object.active_schema_version, "$.result.active_schema_version", 1, 255);
  const supportedOperations = array(object.supported_operations, "$.result.supported_operations", 256).map((item, index) => string(item, `$.result.supported_operations[${index}]`, 128));
  const supportedFeatures = array(object.supported_features, "$.result.supported_features", 256).map((item, index) => string(item, `$.result.supported_features[${index}]`, 128));
  return {
    compatible: boolean(object.compatible, "$.result.compatible"),
    daemonProtocolMin: integer(object.daemon_protocol_min, "$.result.daemon_protocol_min", 1, 255),
    daemonProtocolMax: integer(object.daemon_protocol_max, "$.result.daemon_protocol_max", 1, 255),
    supportedOperations,
    supportedFeatures,
  };
}

export function parseStatus(value: unknown): StatusDto {
  const allowed = ["schema_version", "state", "generation", "last_update", "service", "diagnostic_code", "watcher_health", "runtime", "subscription", "core_update", "rule_set", "dns_split", "capture", "operational"];
  const object = record(value, "$.result", allowed);
  const service = record(object.service, "$.result.service", ["configured_enabled", "effective_enabled", "override"]);
  const override = service.override === null || service.override === undefined
    ? undefined
    : enumeration(service.override, "$.result.service.override", ["wifi_scene"] as const);
  const diagnosticCode = object.diagnostic_code === null || object.diagnostic_code === undefined
    ? undefined
    : enumeration(object.diagnostic_code, "$.result.diagnostic_code", ["config_unavailable", "fail_open_direct", "runtime_degraded", "runtime_backoff", "runtime_circuit_open"] as const);
  const extensionKeys = allowed.slice(6);
  const extension = Object.fromEntries(extensionKeys.map((key) => [key, safeExtension(object[key], `$.result.${key}`)]));
  const schemaVersion = integer(object.schema_version, "$.result.schema_version", 2, 2);
  return {
    schemaVersion: schemaVersion as 2,
    state: enumeration(object.state, "$.result.state", RUNTIME_STATES),
    ...(object.generation === undefined || object.generation === null ? {} : { generation: integer(object.generation, "$.result.generation", 1) }),
    lastUpdate: enumeration(object.last_update, "$.result.last_update", ["never", "succeeded", "failed"] as const),
    service: {
      configuredEnabled: boolean(service.configured_enabled, "$.result.service.configured_enabled"),
      effectiveEnabled: boolean(service.effective_enabled, "$.result.service.effective_enabled"),
      ...(override === undefined ? {} : { override }),
    },
    ...(diagnosticCode === undefined ? {} : { diagnosticCode }),
    extension,
  };
}

export function parseCapability(value: unknown): CapabilityDto {
  const object = record(value, "$.result", ["schema_version", "probe_id", "observed_at_monotonic_ms", "report_digest", "items"]);
  const items = array(object.items, "$.result.items", 256).map((item, index): CapabilityItemDto => {
    const path = `$.result.items[${index}]`;
    const entry = record(item, path, ["key", "status", "reason_code", "requirements", "evidence", "apply_effect"]);
    safeExtension(entry.requirements, `${path}.requirements`);
    safeExtension(entry.evidence, `${path}.evidence`);
    return {
      key: string(entry.key, `${path}.key`, 128),
      status: enumeration(entry.status, `${path}.status`, ["supported", "unsupported", "experimental", "conflict", "unavailable"] as const),
      reasonCode: string(entry.reason_code, `${path}.reason_code`, 128),
      applyEffect: string(entry.apply_effect, `${path}.apply_effect`, 128),
    };
  });
  return { schemaVersion: integer(object.schema_version, "$.result.schema_version", 1, 255), probeId: string(object.probe_id, "$.result.probe_id", 96), observedAtMonotonicMs: integer(object.observed_at_monotonic_ms, "$.result.observed_at_monotonic_ms"), reportDigest: digest(object.report_digest, "$.result.report_digest"), items };
}

export function parseConfig(value: unknown): ConfigDto {
  const object = record(value, "$.result", ["observed_config_digest", "active_config_digest", "candidate_sequence", "watcher_health", "last_reload", "document", "source_status"]);
  const document = record(safeExtension(object.document, "$.result.document"), "$.result.document");
  const sourceStatus = array(object.source_status, "$.result.source_status", 256).map((item, index) => parseSourceStatus(item, `$.result.source_status[${index}]`));
  string(object.watcher_health, "$.result.watcher_health", 64);
  string(object.last_reload, "$.result.last_reload", 64);
  return { observedConfigDigest: digest(object.observed_config_digest, "$.result.observed_config_digest"), activeConfigDigest: digest(object.active_config_digest, "$.result.active_config_digest"), candidateSequence: integer(object.candidate_sequence, "$.result.candidate_sequence"), document, sourceStatus };
}

function optionalInteger(value: unknown, path: string): number | undefined {
  return value === undefined || value === null ? undefined : integer(value, path);
}

export function parseSourceStatus(value: unknown, path = "$.source_status"): SourceStatusDto {
  const object = record(value, path, ["source_id", "health", "last_attempt_wall_seconds", "last_success_wall_seconds", "next_update_wall_seconds", "generation", "accepted", "duplicate", "rejected", "warnings", "subscription_upload_bytes", "subscription_download_bytes", "subscription_total_bytes", "subscription_expire_at", "using_last_known_good", "diagnostic_code"]);
  const lastAttemptWallSeconds = optionalInteger(object.last_attempt_wall_seconds, `${path}.last_attempt_wall_seconds`);
  const lastSuccessWallSeconds = optionalInteger(object.last_success_wall_seconds, `${path}.last_success_wall_seconds`);
  const nextUpdateWallSeconds = optionalInteger(object.next_update_wall_seconds, `${path}.next_update_wall_seconds`);
  const generation = optionalInteger(object.generation, `${path}.generation`);
  const subscriptionUploadBytes = optionalInteger(object.subscription_upload_bytes, `${path}.subscription_upload_bytes`);
  const subscriptionDownloadBytes = optionalInteger(object.subscription_download_bytes, `${path}.subscription_download_bytes`);
  const subscriptionTotalBytes = optionalInteger(object.subscription_total_bytes, `${path}.subscription_total_bytes`);
  const subscriptionExpireAt = optionalInteger(object.subscription_expire_at, `${path}.subscription_expire_at`);
  return {
    sourceId: string(object.source_id, `${path}.source_id`, 96),
    health: enumeration(object.health, `${path}.health`, ["never", "healthy", "degraded", "failed"] as const),
    ...(lastAttemptWallSeconds === undefined ? {} : { lastAttemptWallSeconds }),
    ...(lastSuccessWallSeconds === undefined ? {} : { lastSuccessWallSeconds }),
    ...(nextUpdateWallSeconds === undefined ? {} : { nextUpdateWallSeconds }),
    ...(generation === undefined ? {} : { generation }),
    accepted: integer(object.accepted, `${path}.accepted`),
    duplicate: integer(object.duplicate, `${path}.duplicate`),
    rejected: integer(object.rejected, `${path}.rejected`),
    warnings: integer(object.warnings, `${path}.warnings`),
    ...(subscriptionUploadBytes === undefined ? {} : { subscriptionUploadBytes }),
    ...(subscriptionDownloadBytes === undefined ? {} : { subscriptionDownloadBytes }),
    ...(subscriptionTotalBytes === undefined ? {} : { subscriptionTotalBytes }),
    ...(subscriptionExpireAt === undefined ? {} : { subscriptionExpireAt }),
    usingLastKnownGood: boolean(object.using_last_known_good, `${path}.using_last_known_good`),
    ...(object.diagnostic_code === undefined || object.diagnostic_code === null ? {} : { diagnosticCode: string(object.diagnostic_code, `${path}.diagnostic_code`, 64) }),
  };
}

export function parseConfigSchema(value: unknown): ConfigSchemaDto {
  const object = record(value, "$.result", ["schema_version", "fields", "features"]);
  if (object.features !== undefined) safeExtension(object.features, "$.result.features");
  const fields = array(object.fields, "$.result.fields", 512).map((value, index): ConfigSchemaFieldDto => {
    const path = `$.result.fields[${index}]`;
    const field = record(value, path, ["field_id", "path", "value_type", "default", "title_key", "description_key", "group", "order", "advanced", "experimental", "deprecated", "sensitive", "read_only", "write_only", "apply_impact", "risk_level", "capability_key", "stage", "enum_values", "min", "max", "max_items"]);
    ["default", "description_key", "deprecated", "write_only", "stage", "min", "max", "max_items"].forEach((key) => { if (field[key] !== undefined) safeExtension(field[key], `${path}.${key}`); });
    const options = field.enum_values === undefined ? [] : array(field.enum_values, `${path}.enum_values`, 128).map((item, itemIndex) => string(item, `${path}.enum_values[${itemIndex}]`, 128));
    return {
      id: string(field.field_id, `${path}.field_id`, 128), path: string(field.path, `${path}.path`, 128), valueType: string(field.value_type, `${path}.value_type`, 64), title: string(field.title_key, `${path}.title_key`, 128), group: string(field.group, `${path}.group`, 64), order: integer(field.order, `${path}.order`, 0, 10_000), advanced: boolean(field.advanced, `${path}.advanced`), experimental: boolean(field.experimental, `${path}.experimental`), sensitive: boolean(field.sensitive, `${path}.sensitive`), readOnly: boolean(field.read_only, `${path}.read_only`), applyImpact: string(field.apply_impact, `${path}.apply_impact`, 64), riskLevel: string(field.risk_level, `${path}.risk_level`, 64), ...(typeof field.capability_key === "string" ? { capabilityKey: string(field.capability_key, `${path}.capability_key`, 128) } : {}), options,
    };
  });
  return { schemaVersion: integer(object.schema_version, "$.result.schema_version", 1, 255), fields };
}

export function parseSubscription(value: unknown): SubscriptionDto {
  const object = record(value, "$.subscription", ["id", "name", "configured", "active", "node_count", "auto_candidate_count", "state", "url_redacted"]);
  if (object.url_redacted !== undefined && object.url_redacted !== "[REDACTED]") throw new ValidationError("$.subscription.url_redacted", "URL must be redacted");
  const nodeCount = optionalInteger(object.node_count, "$.subscription.node_count");
  const autoCandidateCount = optionalInteger(object.auto_candidate_count, "$.subscription.auto_candidate_count");
  return {
    id: string(object.id, "$.subscription.id", 96),
    name: string(object.name, "$.subscription.name", 256),
    configured: boolean(object.configured, "$.subscription.configured"),
    active: boolean(object.active, "$.subscription.active"),
    ...(nodeCount === undefined ? {} : { nodeCount }),
    ...(autoCandidateCount === undefined ? {} : { autoCandidateCount }),
    ...(object.state === undefined ? {} : { state: string(object.state, "$.subscription.state", 64) }),
  };
}

export function parseSubscriptionSnapshot(value: unknown): SubscriptionSnapshotDto {
  const object = record(value, "$.result", ["mode", "active_source_ids", "sources", "config_digest"]);
  const sources = array(object.sources, "$.result.sources", 256).map(parseSubscription);
  const activeSourceIds = array(object.active_source_ids, "$.result.active_source_ids", 256).map((item, index) => string(item, `$.result.active_source_ids[${index}]`, 96));
  if (new Set(activeSourceIds).size !== activeSourceIds.length || activeSourceIds.some((id) => !sources.some((source) => source.id === id && source.active))) throw new ValidationError("$.result.active_source_ids", "invalid active source set");
  return { mode: enumeration(object.mode, "$.result.mode", ["single", "merge"] as const), activeSourceIds, sources, configDigest: digest(object.config_digest, "$.result.config_digest") };
}

export function parseNode(value: unknown): NodeDto {
  const object = record(value, "$.node", ["id", "name", "protocol", "latency_ms", "alive", "is_requested", "is_active", "source_ids", "display_territory_code"]);
  const alive = object.alive === undefined || object.alive === null ? undefined : boolean(object.alive, "$.node.alive");
  const displayTerritoryCode = object.display_territory_code === undefined || object.display_territory_code === null
    ? undefined
    : enumeration(object.display_territory_code, "$.node.display_territory_code", territoryCodes);
  return {
    id: string(object.id, "$.node.id", 128),
    name: string(object.name, "$.node.name", 512),
    protocol: enumeration(object.protocol, "$.node.protocol", ["vless", "vmess", "shadowsocks", "trojan", "hysteria2", "tuic", "anytls", "http", "socks"] as const),
    ...(object.latency_ms === undefined || object.latency_ms === null ? {} : { latencyMs: integer(object.latency_ms, "$.node.latency_ms", 0, 600_000) }),
    ...(alive === undefined ? {} : { alive }),
    isRequested: boolean(object.is_requested, "$.node.is_requested"),
    isActive: boolean(object.is_active, "$.node.is_active"),
    sourceIds: array(object.source_ids, "$.node.source_ids", 16).map((item, index) => string(item, `$.node.source_ids[${index}]`, 96)),
    ...(displayTerritoryCode === undefined ? {} : { displayTerritoryCode }),
  };
}

export function parseNodeSelection(value: unknown): NodeSelectionDto {
  const object = record(value, "$.selection", ["version", "intent", "active_terminal", "changed_at"]);
  integer(object.version, "$.selection.version", 2, 2);
  const intentObject = record(object.intent, "$.selection.intent", ["mode", "node_id"]);
  const mode = enumeration(intentObject.mode, "$.selection.intent.mode", ["auto", "manual"] as const);
  const nodeId = intentObject.node_id === undefined ? undefined : string(intentObject.node_id, "$.selection.intent.node_id", 128);
  if ((mode === "auto") !== (nodeId === undefined)) throw new ValidationError("$.selection.intent", "invalid selection intent");
  const terminalObject = record(object.active_terminal, "$.selection.active_terminal", ["kind", "node_id", "reason"]);
  const kind = enumeration(terminalObject.kind, "$.selection.active_terminal.kind", ["node", "direct", "block", "unresolved"] as const);
  const nodeIdValue = terminalObject.node_id === undefined ? undefined : string(terminalObject.node_id, "$.selection.active_terminal.node_id", 128);
  const reasonValue = terminalObject.reason === undefined ? undefined : string(terminalObject.reason, "$.selection.active_terminal.reason", 96);
  if ((kind === "node") !== (nodeIdValue !== undefined) || (kind === "unresolved") !== (reasonValue !== undefined)) {
    throw new ValidationError("$.selection.active_terminal", "invalid active terminal");
  }
  const activeTerminal: ActiveTerminalDto = kind === "node"
    ? { kind, nodeId: nodeIdValue! }
    : kind === "unresolved"
      ? { kind, reason: reasonValue! }
      : { kind };
  return { version: 2, intent: mode === "auto" ? { mode } : { mode, nodeId: nodeId! }, activeTerminal, changedAt: integer(object.changed_at, "$.selection.changed_at") };
}

export function parseNodeList(value: unknown): NodeListSnapshotDto {
  const object = record(value, "$.result", ["nodes", "selection"]);
  return { nodes: array(object.nodes, "$.result.nodes", 10_000).map(parseNode), selection: parseNodeSelection(object.selection) };
}

export function parseNodeDelayList(value: unknown): readonly NodeDelayDto[] {
  const object = record(value, "$.result", ["id", "latency_ms", "results", "selection"]);
  if (object.selection !== undefined) parseNodeSelection(object.selection);
  const hasSingle = object.id !== undefined || object.latency_ms !== undefined;
  const hasMany = object.results !== undefined;
  if (hasSingle === hasMany) throw new ValidationError("$.result", "expected one delay result shape");
  const values = hasSingle ? [{ id: object.id, latency_ms: object.latency_ms }] : array(object.results, "$.result.results", 2_000);
  return values.map((item, index) => {
    const entry = record(item, `$.result.results[${index}]`, ["id", "latency_ms"]);
    const id = string(entry.id, `$.result.results[${index}].id`, 128);
    if (!/^nh1s-[a-f0-9]{16}$/.test(id)) throw new ValidationError(`$.result.results[${index}].id`, "invalid node id");
    return { id, latencyMs: integer(entry.latency_ms, `$.result.results[${index}].latency_ms`, 0, 65_535) };
  });
}

function parseNodeProbeOutcome(value: unknown, path: string): NodeProbeOutcomeDto {
  const entry = record(value, path, ["node_id", "state", "latency_ms", "request_elapsed_us", "completed_at_us"]);
  const id = string(entry.node_id, `${path}.node_id`, 21);
  if (!/^nh1s-[a-f0-9]{16}$/.test(id)) throw new ValidationError(`${path}.node_id`, "invalid node id");
  const state = enumeration(entry.state, `${path}.state`, ["success", "timeout", "unavailable", "protocol_error"] as const);
  const latencyMs = entry.latency_ms === undefined ? undefined : integer(entry.latency_ms, `${path}.latency_ms`, 1, 600_000);
  if ((state === "success") !== (latencyMs !== undefined)) throw new ValidationError(path, "invalid probe outcome");
  const requestElapsedUs = timingUs(entry.request_elapsed_us, `${path}.request_elapsed_us`);
  const completedAtUs = timingUs(entry.completed_at_us, `${path}.completed_at_us`);
  if (requestElapsedUs > completedAtUs) throw new ValidationError(path, "inconsistent probe timing");
  return { id, state, ...(latencyMs === undefined ? {} : { latencyMs }), requestElapsedUs, completedAtUs };
}

export function parseNodeBenchmarkProgress(value: unknown): NodeBenchmarkProgressDto {
  const object = record(value, "$.result", ["operation_id", "phase", "generation", "completed", "candidate_count", "outcome"]);
  const operationId = string(object.operation_id, "$.result.operation_id", 35);
  if (!/^bench_[a-f0-9]{29}$/.test(operationId)) throw new ValidationError("$.result.operation_id", "invalid benchmark operation id");
  const candidateCount = integer(object.candidate_count, "$.result.candidate_count", 1, 64);
  const completed = integer(object.completed, "$.result.completed", 1, candidateCount);
  return {
    operationId,
    phase: enumeration(object.phase, "$.result.phase", ["progress"] as const),
    generation: integer(object.generation, "$.result.generation", 1),
    completed,
    candidateCount,
    outcome: parseNodeProbeOutcome(object.outcome, "$.result.outcome"),
  };
}

function parseNodeBenchmarkFastSelection(value: unknown, path: string): NodeBenchmarkFastSelectionDto {
  const object = record(value, path, ["state", "completed", "candidate_count", "elapsed_us", "selection", "reason"]);
  const state = enumeration(object.state, `${path}.state`, ["pending", "switched", "kept", "deferred", "not_applicable", "superseded", "not_needed"] as const);
  if (state === "pending") {
    if (Object.keys(object).length !== 1) throw new ValidationError(path, "pending fast selection has metrics");
    return { state };
  }
  const candidateCount = integer(object.candidate_count, `${path}.candidate_count`, 1, 64);
  const completed = integer(object.completed, `${path}.completed`, 0, candidateCount);
  const elapsedUs = integer(object.elapsed_us, `${path}.elapsed_us`, 0, state === "switched" ? 3_000_000 : 4_900_000);
  if (state === "switched") {
    if (object.selection === undefined || object.reason !== undefined) throw new ValidationError(path, "invalid switched fast selection");
    return { state, completed, candidateCount, elapsedUs, selection: parseNodeSelection(object.selection) };
  }
  if (state === "deferred") {
    if (object.selection !== undefined) throw new ValidationError(path, "deferred fast selection has selection");
    return {
      state,
      completed,
      candidateCount,
      elapsedUs,
      reason: enumeration(object.reason, `${path}.reason`, ["insufficient_coverage", "current_pending", "no_success", "control_error"] as const),
    };
  }
  if (object.selection !== undefined || object.reason !== undefined) throw new ValidationError(path, "unexpected fast selection payload");
  return { state, completed, candidateCount, elapsedUs };
}

export function parseNodeBenchmarkSelection(value: unknown): NodeBenchmarkSelectionDto {
  const object = record(value, "$.result", ["operation_id", "phase", "generation", "fast_selection"]);
  const operationId = string(object.operation_id, "$.result.operation_id", 35);
  if (!/^bench_[a-f0-9]{29}$/.test(operationId)) throw new ValidationError("$.result.operation_id", "invalid benchmark operation id");
  const fastSelection = parseNodeBenchmarkFastSelection(object.fast_selection, "$.result.fast_selection");
  if (fastSelection.state === "pending") throw new ValidationError("$.result.fast_selection", "selection milestone is pending");
  return {
    operationId,
    phase: enumeration(object.phase, "$.result.phase", ["selection"] as const),
    generation: integer(object.generation, "$.result.generation", 1),
    fastSelection,
  };
}

const MAX_NODE_BENCHMARK_TIMING_US = 60_000_000;

function timingUs(value: unknown, path: string): number {
  return integer(value, path, 0, MAX_NODE_BENCHMARK_TIMING_US);
}

function parseBenchmarkEngineTiming(value: unknown, bootstrapMs: number, elapsedMs: number): BenchmarkEngineTimingDto {
  const path = "$.result.report.timing";
  const object = record(value, path, ["thread_spawn_us", "runtime_init_us", "candidate_dispatch_us", "probe_us", "result_assembly_us", "total_us"]);
  const timing: BenchmarkEngineTimingDto = {
    threadSpawnUs: timingUs(object.thread_spawn_us, `${path}.thread_spawn_us`),
    runtimeInitUs: timingUs(object.runtime_init_us, `${path}.runtime_init_us`),
    candidateDispatchUs: timingUs(object.candidate_dispatch_us, `${path}.candidate_dispatch_us`),
    probeUs: timingUs(object.probe_us, `${path}.probe_us`),
    resultAssemblyUs: timingUs(object.result_assembly_us, `${path}.result_assembly_us`),
    totalUs: timingUs(object.total_us, `${path}.total_us`),
  };
  const accounted = timing.threadSpawnUs + timing.runtimeInitUs + timing.candidateDispatchUs + timing.probeUs + timing.resultAssemblyUs;
  if (accounted > timing.totalUs || Math.floor(timing.totalUs / 1_000) !== elapsedMs || Math.floor((timing.threadSpawnUs + timing.runtimeInitUs) / 1_000) !== bootstrapMs) {
    throw new ValidationError(path, "inconsistent engine timing");
  }
  return timing;
}

function parseBenchmarkProbeSummary(value: unknown, tested: number, succeeded: number, timing: BenchmarkEngineTimingDto): BenchmarkProbeSummaryDto {
  const path = "$.result.report.probe";
  const object = record(value, path, ["first_result_us", "last_result_us", "last_success_us", "completed_within_500ms", "completed_within_1s", "completed_within_2s", "completed_within_3s", "completed_before_cutoff", "cutoff_pending", "cutoff_tail_us"]);
  const optionalTiming = (entry: unknown, field: string) => entry === undefined ? undefined : timingUs(entry, `${path}.${field}`);
  const firstResultUs = optionalTiming(object.first_result_us, "first_result_us");
  const lastResultUs = optionalTiming(object.last_result_us, "last_result_us");
  const lastSuccessUs = optionalTiming(object.last_success_us, "last_success_us");
  const summary: BenchmarkProbeSummaryDto = {
    ...(firstResultUs === undefined ? {} : { firstResultUs }),
    ...(lastResultUs === undefined ? {} : { lastResultUs }),
    ...(lastSuccessUs === undefined ? {} : { lastSuccessUs }),
    completedWithin500ms: integer(object.completed_within_500ms, `${path}.completed_within_500ms`, 0, tested),
    completedWithin1s: integer(object.completed_within_1s, `${path}.completed_within_1s`, 0, tested),
    completedWithin2s: integer(object.completed_within_2s, `${path}.completed_within_2s`, 0, tested),
    completedWithin3s: integer(object.completed_within_3s, `${path}.completed_within_3s`, 0, tested),
    completedBeforeCutoff: integer(object.completed_before_cutoff, `${path}.completed_before_cutoff`, 0, tested),
    cutoffPending: integer(object.cutoff_pending, `${path}.cutoff_pending`, 0, tested),
    cutoffTailUs: timingUs(object.cutoff_tail_us, `${path}.cutoff_tail_us`),
  };
  const counts = [summary.completedWithin500ms, summary.completedWithin1s, summary.completedWithin2s, summary.completedWithin3s, summary.completedBeforeCutoff];
  const hasCompleted = summary.completedBeforeCutoff > 0;
  if (counts.some((count, index) => index > 0 && counts[index - 1]! > count)
    || summary.completedBeforeCutoff + summary.cutoffPending !== tested
    || (summary.firstResultUs !== undefined) !== hasCompleted
    || (summary.lastResultUs !== undefined) !== hasCompleted
    || (summary.lastSuccessUs !== undefined) !== (succeeded > 0)
    || (summary.firstResultUs !== undefined && summary.lastResultUs !== undefined && summary.firstResultUs > summary.lastResultUs)
    || (summary.lastSuccessUs !== undefined && summary.lastResultUs !== undefined && summary.lastSuccessUs > summary.lastResultUs)
    || [summary.firstResultUs, summary.lastResultUs, summary.lastSuccessUs].some((entry) => entry !== undefined && entry > timing.totalUs)
    || summary.cutoffTailUs > timing.probeUs
    || (summary.cutoffPending === 0 && summary.cutoffTailUs !== 0)
    || (summary.lastResultUs !== undefined && summary.lastResultUs + summary.cutoffTailUs > timing.totalUs)) {
    throw new ValidationError(path, "inconsistent probe summary");
  }
  return summary;
}

function parseBenchmarkControlTiming(value: unknown, path: string): BenchmarkControlTimingDto {
  const object = record(value, path, ["intent_load_us", "current_snapshot_us", "decision_us", "target_resolve_us", "selector_apply_us", "final_snapshot_us", "total_us"]);
  const timing: BenchmarkControlTimingDto = {
    intentLoadUs: timingUs(object.intent_load_us, `${path}.intent_load_us`),
    currentSnapshotUs: timingUs(object.current_snapshot_us, `${path}.current_snapshot_us`),
    decisionUs: timingUs(object.decision_us, `${path}.decision_us`),
    targetResolveUs: timingUs(object.target_resolve_us, `${path}.target_resolve_us`),
    selectorApplyUs: timingUs(object.selector_apply_us, `${path}.selector_apply_us`),
    finalSnapshotUs: timingUs(object.final_snapshot_us, `${path}.final_snapshot_us`),
    totalUs: timingUs(object.total_us, `${path}.total_us`),
  };
  const accounted = timing.intentLoadUs + timing.currentSnapshotUs + timing.decisionUs + timing.targetResolveUs + timing.selectorApplyUs + timing.finalSnapshotUs;
  if (accounted > timing.totalUs) throw new ValidationError(path, "inconsistent control timing");
  return timing;
}

function parseBenchmarkTerminalTiming(value: unknown, engineTotalUs: number): BenchmarkTerminalTimingDto {
  const path = "$.result.timing";
  const object = record(value, path, ["admission_us", "worker_reap_us", "fast_control", "terminal_control", "operation_total_us"]);
  const timing: BenchmarkTerminalTimingDto = {
    admissionUs: timingUs(object.admission_us, `${path}.admission_us`),
    workerReapUs: timingUs(object.worker_reap_us, `${path}.worker_reap_us`),
    fastControl: parseBenchmarkControlTiming(object.fast_control, `${path}.fast_control`),
    terminalControl: parseBenchmarkControlTiming(object.terminal_control, `${path}.terminal_control`),
    operationTotalUs: timingUs(object.operation_total_us, `${path}.operation_total_us`),
  };
  if (timing.fastControl.totalUs > timing.operationTotalUs
    || timing.admissionUs + engineTotalUs + timing.workerReapUs + timing.terminalControl.totalUs > timing.operationTotalUs) {
    throw new ValidationError(path, "inconsistent operation timing");
  }
  return timing;
}

export function parseNodeBenchmarkResult(value: unknown): NodeBenchmarkResultDto {
  const object = record(value, "$.result", ["operation_id", "phase", "report", "selection", "fast_selection", "timing"]);
  const operationId = string(object.operation_id, "$.result.operation_id", 35);
  if (!/^bench_[a-f0-9]{29}$/.test(operationId)) throw new ValidationError("$.result.operation_id", "invalid benchmark operation id");
  const phase = enumeration(object.phase, "$.result.phase", ["completed"] as const);
  const report = record(object.report, "$.result.report", ["status", "trigger", "generation", "bootstrap_ms", "elapsed_ms", "timing", "probe", "tested", "succeeded", "timed_out", "failed", "diagnostic", "nodes"]);
  const nodes = array(report.nodes, "$.result.report.nodes", 64).map((item, index) => parseNodeProbeOutcome(item, `$.result.report.nodes[${index}]`));
  const bootstrapMs = integer(report.bootstrap_ms, "$.result.report.bootstrap_ms", 0, 4_900);
  const elapsedMs = integer(report.elapsed_ms, "$.result.report.elapsed_ms", 0, 4_900);
  const engineTiming = parseBenchmarkEngineTiming(report.timing, bootstrapMs, elapsedMs);
  const tested = integer(report.tested, "$.result.report.tested", 0, 64);
  const succeeded = integer(report.succeeded, "$.result.report.succeeded", 0, 64);
  const timedOut = integer(report.timed_out, "$.result.report.timed_out", 0, 64);
  const failed = integer(report.failed, "$.result.report.failed", 0, 64);
  const probe = parseBenchmarkProbeSummary(report.probe, tested, succeeded, engineTiming);
  const parsedReport: NodeBenchmarkReportDto = {
    status: enumeration(report.status, "$.result.report.status", ["success", "partial", "failed", "internal_error", "superseded"] as const),
    trigger: enumeration(report.trigger, "$.result.report.trigger", ["manual", "periodic"] as const),
    generation: integer(report.generation, "$.result.report.generation", 1),
    bootstrapMs,
    elapsedMs,
    timing: engineTiming,
    probe,
    tested,
    succeeded,
    timedOut,
    failed,
    ...(report.diagnostic === undefined ? {} : { diagnostic: enumeration(report.diagnostic, "$.result.report.diagnostic", ["unauthorized"] as const) }),
    nodes,
  };
  const observedSucceeded = nodes.filter((node) => node.state === "success").length;
  const observedTimedOut = nodes.filter((node) => node.state === "timeout").length;
  if (parsedReport.tested !== nodes.length || parsedReport.succeeded !== observedSucceeded || parsedReport.timedOut !== observedTimedOut || parsedReport.failed !== nodes.length - observedSucceeded - observedTimedOut) throw new ValidationError("$.result.report", "inconsistent benchmark counts");
  const selection = object.selection === undefined || object.selection === null ? undefined : parseNodeSelection(object.selection);
  const fastSelection = parseNodeBenchmarkFastSelection(object.fast_selection, "$.result.fast_selection");
  if (fastSelection.state === "pending") throw new ValidationError("$.result.fast_selection", "completed benchmark has pending fast selection");
  const timing = parseBenchmarkTerminalTiming(object.timing, engineTiming.totalUs);
  return { operationId, phase, report: parsedReport, ...(selection === undefined ? {} : { selection }), fastSelection, timing };
}

export function parseNodeBenchmarkAck(value: unknown): NodeBenchmarkAckDto {
  const object = record(value, "$.result", ["operation_id", "phase", "joined_existing", "trigger", "candidate_count", "fast_selection_earliest_ms", "fast_selection_latest_ms", "fast_selection_deadline_ms", "probe_cutoff_ms", "deadline_ms", "fast_selection"]);
  const operationId = string(object.operation_id, "$.result.operation_id", 35);
  if (!/^bench_[a-f0-9]{29}$/.test(operationId)) throw new ValidationError("$.result.operation_id", "invalid benchmark operation id");
  const probeCutoffMs = integer(object.probe_cutoff_ms, "$.result.probe_cutoff_ms", 4500, 4500);
  const deadlineMs = integer(object.deadline_ms, "$.result.deadline_ms", 4900, 4900);
  const fastSelectionEarliestMs = integer(object.fast_selection_earliest_ms, "$.result.fast_selection_earliest_ms", 2000, 2000);
  const fastSelectionLatestMs = integer(object.fast_selection_latest_ms, "$.result.fast_selection_latest_ms", 2800, 2800);
  const fastSelectionDeadlineMs = integer(object.fast_selection_deadline_ms, "$.result.fast_selection_deadline_ms", 3000, 3000);
  return {
    operationId,
    phase: enumeration(object.phase, "$.result.phase", ["running"] as const),
    joinedExisting: boolean(object.joined_existing, "$.result.joined_existing"),
    trigger: enumeration(object.trigger, "$.result.trigger", ["manual", "periodic"] as const),
    candidateCount: integer(object.candidate_count, "$.result.candidate_count", 1, 64),
    fastSelectionEarliestMs: fastSelectionEarliestMs as 2000,
    fastSelectionLatestMs: fastSelectionLatestMs as 2800,
    fastSelectionDeadlineMs: fastSelectionDeadlineMs as 3000,
    probeCutoffMs: probeCutoffMs as 4500,
    deadlineMs: deadlineMs as 4900,
    fastSelection: parseNodeBenchmarkFastSelection(object.fast_selection, "$.result.fast_selection"),
  };
}

export function parseApplication(value: unknown): ApplicationDto {
  const object = record(value, "$.application", ["package_name", "uid", "mode", "shared_uid"]);
  const packageName = string(object.package_name, "$.application.package_name", 256);
  if (!/^[A-Za-z0-9_.-]+$/.test(packageName)) throw new ValidationError("$.application.package_name", "invalid package name");
  return { packageName, uid: integer(object.uid, "$.application.uid", 0, 4_294_967_295), mode: enumeration(object.mode, "$.application.mode", ["all", "blacklist", "whitelist"] as const), sharedUid: boolean(object.shared_uid, "$.application.shared_uid") };
}

export function parseApplicationList(value: unknown): readonly ApplicationDto[] {
  const object = record(value, "$.result", ["applications", "items"]);
  return array(object.applications ?? object.items, "$.result.applications", 2_000).map(parseApplication);
}

export function parseTraffic(value: unknown): TrafficDto {
  const object = record(value, "$.traffic", ["kind", "sample", "interval_seconds"]);
  if (object.kind !== undefined && object.kind !== "traffic") throw new ValidationError("$.traffic.kind", "invalid traffic kind");
  const sample = record(object.sample, "$.traffic.sample", ["up", "down"]);
  return { up: integer(sample.up, "$.traffic.sample.up"), down: integer(sample.down, "$.traffic.sample.down"), intervalSeconds: integer(object.interval_seconds, "$.traffic.interval_seconds", 1, 60) };
}

export function parseLogs(value: unknown): readonly LogEntryDto[] {
  const object = record(value, "$.logs", ["entries", "channel", "newest_first"]);
  return array(object.entries, "$.logs.entries", 128).map((value, index) => {
    const entry = record(value, `$.logs.entries[${index}]`, ["seq", "kind", "payload", "channel", "raw"]);
    const payload = record(entry.payload, `$.logs.entries[${index}].payload`);
    const channel = enumeration(entry.channel, `$.logs.entries[${index}].channel`, ["service", "subscription", "core"] as const);
    const kind = typeof payload.kind === "string" ? payload.kind : string(entry.kind, `$.logs.entries[${index}].kind`, 32);
    const message = typeof payload.message === "string" ? payload.message : typeof payload.state === "string" ? payload.state : kind;
    const time = typeof payload.timestamp === "string" ? payload.timestamp : typeof payload.time === "string" ? payload.time : "--";
    return { id: String(integer(entry.seq, `$.logs.entries[${index}].seq`, 1)), channel, kind: kind.slice(0, 96), message: message.slice(0, 512), time: time.slice(0, 64), raw: string(entry.raw, `$.logs.entries[${index}].raw`, 16 * 1024) };
  });
}

export function parseRuntimeMetrics(value: unknown): RuntimeMetricsDto {
  const object = record(value, "$.metrics", ["schema_version", "runtime_state", "generation", "uptime_seconds", "core", "traffic", "outbound"]);
  integer(object.schema_version, "$.metrics.schema_version", 1, 1);
  const core = object.core === null || object.core === undefined ? undefined : record(object.core, "$.metrics.core", ["pid", "cpu_percent", "memory_rss_bytes"]);
  const traffic = record(object.traffic, "$.metrics.traffic", ["upload_bytes", "download_bytes"]);
  const outbound = record(object.outbound, "$.metrics.outbound", ["interface", "local_address", "public_ip"]);
  const optionalNumber = (candidate: unknown, path: string): number | undefined => candidate === null || candidate === undefined ? undefined : integer(candidate, path);
  const optionalText = (candidate: unknown, path: string): string | undefined => candidate === null || candidate === undefined ? undefined : string(candidate, path, 128);
  const generation = optionalNumber(object.generation, "$.metrics.generation");
  const uploadBytes = optionalNumber(traffic.upload_bytes, "$.metrics.traffic.upload_bytes");
  const downloadBytes = optionalNumber(traffic.download_bytes, "$.metrics.traffic.download_bytes");
  const interfaceName = optionalText(outbound.interface, "$.metrics.outbound.interface");
  const localAddress = optionalText(outbound.local_address, "$.metrics.outbound.local_address");
  const publicIp = optionalText(outbound.public_ip, "$.metrics.outbound.public_ip");
  const coreMetrics = core ? (() => {
    const cpuPercent = core.cpu_percent === null || core.cpu_percent === undefined ? undefined : finiteNumber(core.cpu_percent, "$.metrics.core.cpu_percent", 0, 10_000);
    const memoryRssBytes = optionalNumber(core.memory_rss_bytes, "$.metrics.core.memory_rss_bytes");
    return { pid: integer(core.pid, "$.metrics.core.pid", 1), ...(cpuPercent === undefined ? {} : { cpuPercent }), ...(memoryRssBytes === undefined ? {} : { memoryRssBytes }) };
  })() : undefined;
  return {
    runtimeState: string(object.runtime_state, "$.metrics.runtime_state", 32),
    uptimeSeconds: integer(object.uptime_seconds, "$.metrics.uptime_seconds"),
    ...(generation === undefined ? {} : { generation }),
    ...(coreMetrics === undefined ? {} : { core: coreMetrics }),
    ...(uploadBytes === undefined ? {} : { uploadBytes }),
    ...(downloadBytes === undefined ? {} : { downloadBytes }),
    ...(interfaceName === undefined ? {} : { interface: interfaceName }),
    ...(localAddress === undefined ? {} : { localAddress }),
    ...(publicIp === undefined ? {} : { publicIp }),
  };
}

export function parseEventFrame(value: unknown): EventFrameDto {
  const object = record(value, "$", ["version", "request_id", "sequence", "kind", "payload", "error"]);
  if (integer(object.version, "$.version", 1, 255) !== 5) throw new ValidationError("$.version", "unsupported protocol");
  const requestId = string(object.request_id, "$.request_id", 96);
  const sequence = integer(object.sequence, "$.sequence", 1);
  const kind = enumeration(object.kind, "$.kind", ["item", "end", "error"] as const);
  if (kind === "end") {
    if (object.payload !== undefined || object.error !== undefined) throw new ValidationError("$", "invalid end frame");
    return { type: "end", requestId, sequence };
  }
  if (kind === "error") {
    if (object.payload !== undefined) throw new ValidationError("$.payload", "unexpected payload");
    const errorObject = record(object.error, "$.error", ["code", "message", "details"]);
    const error: ErrorDto = { code: string(errorObject.code, "$.error.code", 96), message: string(errorObject.message, "$.error.message", 1024), ...(errorObject.details === undefined ? {} : { details: safeExtension(errorObject.details, "$.error.details") }) };
    return { type: "error", requestId, sequence, error };
  }
  if (object.error !== undefined) throw new ValidationError("$.error", "unexpected error");
  const payload = record(object.payload, "$.payload");
  const payloadKind = enumeration(payload.kind, "$.payload.kind", EVENT_KINDS);
  const validated = record(safeExtension(payload, "$.payload"), "$.payload");
  return { type: "item", requestId, sequence, payload: { kind: payloadKind, value: validated, ...(payloadKind === "traffic" ? { traffic: parseTraffic(payload) } : {}) } };
}

export type OperationalDomain = "connections" | "logs" | "topology" | "ruleset" | "version" | "diagnostics";
export function parseOperational(value: unknown, domain: OperationalDomain): Readonly<Record<string, unknown>> {
  const object = record(value, `$.${domain}`);
  return record(safeExtension(object, `$.${domain}`), `$.${domain}`);
}

export function optionalDisplay(value: unknown, path: string): string | undefined { return optionalString(value, path, 512); }
