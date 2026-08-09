import { computed, reactive, readonly, shallowRef, type ComputedRef, type Ref } from "vue";

import type { HostCapability } from "@/bridge/host";
import type { ApplicationDto, ConfigDto, HelloDto, NodeDto, StatusDto, SubscriptionDto } from "@/model/dto";

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
  readonly subscriptionsById: Ref<Readonly<Record<string, SubscriptionDto>>>;
  readonly subscriptionOrder: Ref<readonly string[]>;
  readonly applicationsByPackage: Ref<Readonly<Record<string, ApplicationDto>>>;
  readonly applicationOrder: Ref<readonly string[]>;
  readonly replaceNodes: (nodes: readonly NodeDto[]) => void;
  readonly upsertNode: (node: NodeDto) => void;
  readonly removeNode: (id: string) => void;
  readonly replaceSubscriptions: (items: readonly SubscriptionDto[]) => void;
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
  const subscriptionsById = shallowRef<Readonly<Record<string, SubscriptionDto>>>({});
  const subscriptionOrder = shallowRef<readonly string[]>([]);
  const applicationsByPackage = shallowRef<Readonly<Record<string, ApplicationDto>>>({});
  const applicationOrder = shallowRef<readonly string[]>([]);
  return {
    nodesById, nodeOrder, subscriptionsById, subscriptionOrder, applicationsByPackage, applicationOrder,
    replaceNodes: (items) => replaceEntity(nodesById, nodeOrder, items.map((item) => ({ ...item }))),
    upsertNode: (item) => upsertEntity(nodesById, nodeOrder, { ...item }),
    removeNode: (id) => removeEntity(nodesById, nodeOrder, id),
    replaceSubscriptions: (items) => replaceEntity(subscriptionsById, subscriptionOrder, items.map((item) => ({ ...item }))),
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
      subscriptionsById.value = {}; subscriptionOrder.value = [];
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
