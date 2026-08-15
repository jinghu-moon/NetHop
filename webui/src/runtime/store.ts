import { computed, reactive, readonly, shallowRef, type ComputedRef, type Ref } from "vue";

import type { HostCapability } from "@/bridge/host";
import type { ApplicationDto, ConfigDto, HelloDto, NodeBenchmarkFastSelectionDto, NodeDto, NodeListSnapshotDto, NodeProbeOutcomeDto, NodeProbeStateDto, NodeSelectionDto, StatusDto, SubscriptionDto, SubscriptionModeDto, SubscriptionSnapshotDto } from "@/model/dto";

export type SessionPhase = "idle" | "connecting" | "live" | "stale" | "incompatible" | "unavailable" | "closed";

export interface SessionStore {
  readonly phase: Ref<SessionPhase>;
  readonly hello: Ref<HelloDto | undefined>;
  readonly host: Ref<HostCapability | undefined>;
  readonly status: Ref<StatusDto | undefined>;
  readonly setPhase: (phase: SessionPhase) => void;
  readonly setHello: (hello: HelloDto) => void;
  readonly setHost: (host: HostCapability) => void;
  readonly setStatus: (status: StatusDto) => void;
  readonly reset: () => void;
}

export function createSessionStore(): SessionStore {
  const phase = shallowRef<SessionPhase>("idle");
  const hello = shallowRef<HelloDto>();
  const host = shallowRef<HostCapability>();
  const status = shallowRef<StatusDto>();
  return {
    phase,
    hello,
    host,
    status,
    setPhase: (value) => { phase.value = value; },
    setHello: (value) => { hello.value = value; },
    setHost: (value) => { host.value = value; },
    setStatus: (value) => { status.value = value; },
    reset: () => { phase.value = "idle"; hello.value = undefined; host.value = undefined; status.value = undefined; },
  };
}

export interface RuntimeStore {
  readonly nodesById: Ref<Readonly<Record<string, NodeDto>>>;
  readonly nodeOrder: Ref<readonly string[]>;
  readonly selection: Ref<NodeSelectionDto | undefined>;
  readonly nodeProbeStates: Ref<Readonly<Record<string, NodeProbeStateDto | "measuring">>>;
  readonly nodeBenchmarkFastSelection: Ref<NodeBenchmarkFastSelectionDto | undefined>;
  readonly subscriptionsById: Ref<Readonly<Record<string, SubscriptionDto>>>;
  readonly subscriptionOrder: Ref<readonly string[]>;
  readonly subscriptionMode: Ref<SubscriptionModeDto | undefined>;
  readonly activeSourceIds: Ref<readonly string[]>;
  readonly subscriptionConfigDigest: Ref<string | undefined>;
  readonly applicationsByPackage: Ref<Readonly<Record<string, ApplicationDto>>>;
  readonly applicationOrder: Ref<readonly string[]>;
  readonly replaceNodes: (nodes: readonly NodeDto[]) => void;
  readonly loadNodeSnapshot: (snapshot: NodeListSnapshotDto) => void;
  readonly setSelection: (selection: NodeSelectionDto) => void;
  readonly upsertNode: (node: NodeDto) => void;
  readonly removeNode: (id: string) => void;
  readonly beginNodeBenchmark: (nodeIds: readonly string[]) => void;
  readonly applyNodeProbeOutcome: (outcome: NodeProbeOutcomeDto) => void;
  readonly setNodeBenchmarkFastSelection: (state: NodeBenchmarkFastSelectionDto) => void;
  readonly finishNodeBenchmark: (outcomes: readonly NodeProbeOutcomeDto[], fastSelection?: NodeBenchmarkFastSelectionDto) => void;
  readonly clearNodeProbeStates: () => void;
  readonly replaceSubscriptions: (items: readonly SubscriptionDto[]) => void;
  readonly loadSubscriptionSnapshot: (snapshot: SubscriptionSnapshotDto) => void;
  readonly upsertSubscription: (item: SubscriptionDto) => void;
  readonly removeSubscription: (id: string) => void;
  readonly replaceApplications: (items: readonly ApplicationDto[]) => void;
  readonly upsertApplication: (item: ApplicationDto) => void;
  readonly removeApplication: (packageName: string) => void;
  readonly reset: () => void;
}

function replaceEntity<T extends { readonly id: string }>(target: Ref<Readonly<Record<string, T>>>, order: Ref<readonly string[]>, items: readonly T[]): void {
  const next: Record<string, T> = {};
  const ids: string[] = [];
  items.forEach((item) => { next[item.id] = item; ids.push(item.id); });
  target.value = next;
  order.value = ids;
}

function upsertEntity<T extends { readonly id: string }>(target: Ref<Readonly<Record<string, T>>>, order: Ref<readonly string[]>, item: T): void {
  if (target.value[item.id] === item) return;
  target.value = { ...target.value, [item.id]: item };
  if (!order.value.includes(item.id)) order.value = [...order.value, item.id];
}

function removeEntity<T>(target: Ref<Readonly<Record<string, T>>>, order: Ref<readonly string[]>, id: string): void {
  if (!(id in target.value)) return;
  const next = { ...target.value };
  delete next[id];
  target.value = next;
  order.value = order.value.filter((value) => value !== id);
}

export function createRuntimeStore(): RuntimeStore {
  const nodesById = shallowRef<Readonly<Record<string, NodeDto>>>({});
  const nodeOrder = shallowRef<readonly string[]>([]);
  const selection = shallowRef<NodeSelectionDto>();
  const nodeProbeStates = shallowRef<Readonly<Record<string, NodeProbeStateDto | "measuring">>>({});
  const nodeBenchmarkFastSelection = shallowRef<NodeBenchmarkFastSelectionDto>();
  const subscriptionsById = shallowRef<Readonly<Record<string, SubscriptionDto>>>({});
  const subscriptionOrder = shallowRef<readonly string[]>([]);
  const subscriptionMode = shallowRef<SubscriptionModeDto>();
  const activeSourceIds = shallowRef<readonly string[]>([]);
  const subscriptionConfigDigest = shallowRef<string>();
  const applicationsByPackage = shallowRef<Readonly<Record<string, ApplicationDto>>>({});
  const applicationOrder = shallowRef<readonly string[]>([]);
  return {
    nodesById, nodeOrder, selection, nodeProbeStates, nodeBenchmarkFastSelection, subscriptionsById, subscriptionOrder, subscriptionMode, activeSourceIds, subscriptionConfigDigest, applicationsByPackage, applicationOrder,
    replaceNodes: (items) => { replaceEntity(nodesById, nodeOrder, items.map((item) => ({ ...item }))); nodeProbeStates.value = {}; nodeBenchmarkFastSelection.value = undefined; },
    loadNodeSnapshot: (snapshot) => {
      replaceEntity(nodesById, nodeOrder, snapshot.nodes.map((item) => ({ ...item })));
      selection.value = snapshot.selection;
      nodeProbeStates.value = {};
      nodeBenchmarkFastSelection.value = undefined;
    },
    setSelection: (value) => { selection.value = value; },
    upsertNode: (item) => upsertEntity(nodesById, nodeOrder, { ...item }),
    removeNode: (id) => removeEntity(nodesById, nodeOrder, id),
    beginNodeBenchmark: (nodeIds) => {
      nodeProbeStates.value = Object.fromEntries(nodeIds.filter((id) => id in nodesById.value).map((id) => [id, "measuring" as const]));
      nodeBenchmarkFastSelection.value = { state: "pending" };
    },
    applyNodeProbeOutcome: (outcome) => {
      const node = nodesById.value[outcome.id];
      if (!node) return;
      if (outcome.state === "success" && outcome.latencyMs !== undefined) upsertEntity(nodesById, nodeOrder, { ...node, latencyMs: outcome.latencyMs });
      else {
        const { latencyMs: _latencyMs, ...withoutLatency } = node;
        upsertEntity(nodesById, nodeOrder, withoutLatency);
      }
      nodeProbeStates.value = { ...nodeProbeStates.value, [outcome.id]: outcome.state };
    },
    setNodeBenchmarkFastSelection: (state) => { nodeBenchmarkFastSelection.value = state; },
    finishNodeBenchmark: (outcomes, fastSelection) => {
      const states: Record<string, NodeProbeStateDto> = {};
      for (const outcome of outcomes) {
        const node = nodesById.value[outcome.id];
        if (!node) continue;
        if (outcome.state === "success" && outcome.latencyMs !== undefined) upsertEntity(nodesById, nodeOrder, { ...node, latencyMs: outcome.latencyMs });
        else {
          const { latencyMs: _latencyMs, ...withoutLatency } = node;
          upsertEntity(nodesById, nodeOrder, withoutLatency);
        }
        states[outcome.id] = outcome.state;
      }
      nodeProbeStates.value = states;
      if (fastSelection !== undefined) nodeBenchmarkFastSelection.value = fastSelection;
    },
    clearNodeProbeStates: () => { nodeProbeStates.value = {}; nodeBenchmarkFastSelection.value = undefined; },
    replaceSubscriptions: (items) => replaceEntity(subscriptionsById, subscriptionOrder, items.map((item) => ({ ...item }))),
    loadSubscriptionSnapshot: (snapshot) => {
      replaceEntity(subscriptionsById, subscriptionOrder, snapshot.sources.map((item) => ({ ...item })));
      subscriptionMode.value = snapshot.mode;
      activeSourceIds.value = snapshot.activeSourceIds;
      subscriptionConfigDigest.value = snapshot.configDigest;
    },
    upsertSubscription: (item) => upsertEntity(subscriptionsById, subscriptionOrder, { ...item }),
    removeSubscription: (id) => removeEntity(subscriptionsById, subscriptionOrder, id),
    replaceApplications: (items) => {
      const next: Record<string, ApplicationDto> = {};
      const ids: string[] = [];
      items.forEach((item) => { next[item.packageName] = item; ids.push(item.packageName); });
      applicationsByPackage.value = next;
      applicationOrder.value = ids;
    },
    upsertApplication: (item) => {
      applicationsByPackage.value = { ...applicationsByPackage.value, [item.packageName]: { ...item } };
      if (!applicationOrder.value.includes(item.packageName)) applicationOrder.value = [...applicationOrder.value, item.packageName];
    },
    removeApplication: (packageName) => removeEntity(applicationsByPackage, applicationOrder, packageName),
    reset: () => {
      nodesById.value = {}; nodeOrder.value = [];
      selection.value = undefined; nodeProbeStates.value = {}; nodeBenchmarkFastSelection.value = undefined;
      subscriptionsById.value = {}; subscriptionOrder.value = [];
      subscriptionMode.value = undefined; activeSourceIds.value = []; subscriptionConfigDigest.value = undefined;
      applicationsByPackage.value = {}; applicationOrder.value = [];
    },
  };
}

export interface ConfigDraftStore {
  readonly active: Ref<ConfigDto | undefined>;
  readonly draft: Ref<Readonly<Record<string, unknown>> | undefined>;
  readonly baseDigest: Ref<string | undefined>;
  readonly dirty: ComputedRef<boolean>;
  readonly load: (config: ConfigDto) => void;
  readonly edit: (document: Readonly<Record<string, unknown>>) => void;
  readonly discard: () => void;
  readonly markApplied: (config: ConfigDto) => void;
}

function stableJson(value: unknown): string {
  return JSON.stringify(value, (_key, current: unknown) => {
    if (!current || typeof current !== "object" || Array.isArray(current)) return current;
    return Object.fromEntries(Object.entries(current as Record<string, unknown>).sort(([a], [b]) => a.localeCompare(b)));
  });
}

export function createConfigDraftStore(): ConfigDraftStore {
  const active = shallowRef<ConfigDto>();
  const draft = shallowRef<Readonly<Record<string, unknown>>>();
  const baseDigest = shallowRef<string>();
  const dirty = computed(() => draft.value !== undefined && active.value !== undefined && stableJson(draft.value) !== stableJson(active.value.document));
  return {
    active, draft, baseDigest, dirty,
    load: (config) => { active.value = config; draft.value = config.document; baseDigest.value = config.activeConfigDigest; },
    edit: (document) => { draft.value = document; },
    discard: () => { draft.value = active.value?.document; },
    markApplied: (config) => { active.value = config; draft.value = config.document; baseDigest.value = config.activeConfigDigest; },
  };
}

export function createUiStores(): { readonly session: SessionStore; readonly runtime: RuntimeStore; readonly config: ConfigDraftStore } {
  return { session: createSessionStore(), runtime: createRuntimeStore(), config: createConfigDraftStore() };
}

export const uiStores = createUiStores();

export function readonlyStore<T extends object>(store: T): Readonly<T> {
  return readonly(reactive(store)) as Readonly<T>;
}
