<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, ref } from "vue";
import { useDebounce } from "@vueuse/core";
import ApplicationCategoryDropdown from "@/components/applications/ApplicationCategoryDropdown.vue";
import ApplicationSearch from "@/components/applications/ApplicationSearch.vue";
import ApplicationSortDropdown from "@/components/applications/ApplicationSortDropdown.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";
import PageState from "@/components/ui/feedback/PageState.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import Switch from "@/components/ui/primitives/Switch.vue";
import Button from "@/components/ui/primitives/Button.vue";
import Tag from "@/components/ui/primitives/Tag.vue";
import PullRefresh from "@/components/ui/feedback/PullRefresh.vue";
import { useHost } from "@/bridge/context";
import { BridgeError } from "@/bridge/command";
import ApplicationIcon from "@/components/applications/ApplicationIcon.vue";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { readPackages } from "@/bridge/package-adapter";
import { validatedQuery } from "@/model/client";
import { parseConfig } from "@/model/dto";
import { buildApplicationPolicyDocument, buildApplicationPolicyMutation, readApplicationPolicy, type ApplicationMode } from "@/model/application-policy";
import { uiStores } from "@/runtime/store";
import { createOperationStore } from "@/runtime/operation";
import { SearchIndex } from "@/runtime/search-index";
import type { PackageInfo } from "@/bridge/host";
import { hasApplicationSortData, parseApplicationSort, prioritizeSelected, serializeApplicationSort, sortApplications, type ApplicationSort, type ApplicationSortField } from "@/model/application-sort";
import { useUiPreference } from "@/runtime/storage";

interface AppRow extends PackageInfo { readonly selected: boolean; readonly sharedCount: number }
const host = useHost();
const loading = ref(false);
const error = ref("");
const query = ref("");
const debouncedQuery = useDebounce(query, 120);
const category = ref<"all" | "user" | "system">("user");
const mode = ref<ApplicationMode>("all");
const committedMode = ref<ApplicationMode>("all");
const slideDirection = ref<"forward" | "backward">("forward");
const modeOptions = [{ value: "all", label: "全部应用" }, { value: "blacklist", label: "黑名单" }, { value: "whitelist", label: "白名单" }];
const categoryOptions = [{ value: "all", label: "全部应用" }, { value: "user", label: "用户应用" }, { value: "system", label: "系统应用" }] as const;
const packages = ref<readonly PackageInfo[]>([]);
const sortPreference = useUiPreference("application-sort", "name:asc");
const selectedFirstPreference = useUiPreference("application-selected-first", "false");
const selectedFirst = computed(() => selectedFirstPreference.value === "true");
const sort = computed<ApplicationSort>(() => parseApplicationSort(sortPreference.value));
const sortFields: readonly { field: ApplicationSortField; label: string }[] = [
  { field: "name", label: "名称" },
  { field: "updatedAt", label: "更新时间" },
  { field: "storage", label: "存储占用" },
  { field: "lastUsed", label: "最近使用时间" },
];
const availableSortFields = computed(() => sortFields.filter((item) => hasApplicationSortData(packages.value, item.field)));
const selected = ref<ReadonlySet<string>>(new Set());
const committedSelected = ref<ReadonlySet<string>>(new Set());
const saving = ref(false);
const refreshing = ref(false);
const operations = createOperationStore();
const index = new SearchIndex();
let saveRequested = false;
let saveTimer: ReturnType<typeof setTimeout> | undefined;
let firstQueuedAt = 0;

const rows = computed<readonly AppRow[]>(() => {
  const uidCounts = new Map<number, number>(); packages.value.forEach((item) => uidCounts.set(item.uid, (uidCounts.get(item.uid) ?? 0) + 1));
  const matching = new Set(index.query(debouncedQuery.value, 2_000));
  const result = sortApplications(packages.value, sort.value).filter((item) => matching.has(item.packageName) && (category.value === "all" || (category.value === "system") === item.isSystem)).map((item) => ({ ...item, selected: selected.value.has(item.packageName), sharedCount: uidCounts.get(item.uid) ?? 1 }));
  return selectedFirst.value ? prioritizeSelected(result, (item) => item.selected) : result;
});
const dirty = computed(() => mode.value !== committedMode.value || (mode.value !== "all" && !sameSet(selected.value, committedSelected.value)));
const selectedCount = computed(() => selected.value.size);
const candidateDocument = computed(() => buildApplicationPolicyDocument(uiStores.config.active.value?.document ?? {}, mode.value, selected.value));
const missingSelection = computed(() => {
  if (mode.value === "all") return false;
  const application = candidateDocument.value.applications;
  const values = application && typeof application === "object" && !Array.isArray(application) ? (application as Record<string, unknown>).targets : undefined;
  return !Array.isArray(values) || values.length === 0;
});
const selectionEffect = computed(() => mode.value === "blacklist" ? "选中应用直连" : "仅选中应用代理");
const transitionName = computed(() => `application-panel-${slideDirection.value}`);

function sameSet(left: ReadonlySet<string>, right: ReadonlySet<string>): boolean {
  return left.size === right.size && [...left].every((value) => right.has(value));
}
function applyPolicy(document: Readonly<Record<string, unknown>>): void {
  const policy = readApplicationPolicy(document);
  mode.value = policy.mode;
  committedMode.value = policy.mode;
  selected.value = new Set(policy.packages);
  committedSelected.value = new Set(policy.packages);
}
async function load(): Promise<void> {
  if (loading.value) return;
  loading.value = true; error.value = "";
  try {
    const [config, packageList] = await Promise.all([
      validatedQuery(host, { id: "config.get" }, parseConfig),
      readPackages(host, "all").then((items) => items.slice(0, 2_000)),
    ]);
    uiStores.config.load(config);
    applyPolicy(config.document);
    packages.value = packageList;
    if (!hasApplicationSortData(packageList, sort.value.field)) {
      sortPreference.value = serializeApplicationSort({ field: "name", direction: sort.value.direction });
    }
    index.clear();
    packages.value.forEach((item) => index.upsert(item.packageName, item.appLabel, item.packageName));
  } catch { error.value = "宿主无法读取应用包信息"; }
  finally { loading.value = false; }
}
function changeMode(context: { value: string | number }): void {
  const next = context.value;
  if (next !== "all" && next !== "blacklist" && next !== "whitelist") return;
  const currentIndex = modeOptions.findIndex((item) => item.value === mode.value);
  const nextIndex = modeOptions.findIndex((item) => item.value === next);
  slideDirection.value = nextIndex >= currentIndex ? "forward" : "backward";
  mode.value = next;
  queuePolicySave();
}
function updateSort(value: ApplicationSort): void {
  sortPreference.value = serializeApplicationSort(value);
}
function updateSelectedFirst(value: boolean): void {
  selectedFirstPreference.value = value ? "true" : "false";
}
async function pullRefresh(): Promise<void> { refreshing.value = true; try { await load(); } finally { refreshing.value = false; } }
function toggle(row: AppRow, checked: unknown): void {
  if (row.uid === 0) return;
  const next = new Set(selected.value);
  const related = packages.value.filter((item) => item.uid === row.uid).map((item) => item.packageName);
  if (checked === true || checked === "true") related.forEach((item) => next.add(item)); else related.forEach((item) => next.delete(item));
  selected.value = next;
  queuePolicySave();
}
function selectVisible(): void {
  const next = new Set(selected.value);
  rows.value.filter((item) => item.uid !== 0).forEach((item) => packages.value.filter((candidate) => candidate.uid === item.uid).forEach((candidate) => next.add(candidate.packageName)));
  selected.value = next;
  queuePolicySave();
}
function clearVisible(): void {
  const next = new Set(selected.value);
  rows.value.forEach((item) => packages.value.filter((candidate) => candidate.uid === item.uid).forEach((candidate) => next.delete(candidate.packageName)));
  selected.value = next;
  queuePolicySave();
}

function responseDigest(value: unknown): string | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const result = (value as { result?: unknown }).result;
  if (!result || typeof result !== "object" || Array.isArray(result)) return undefined;
  const digest = (result as { observed_config_digest?: unknown }).observed_config_digest;
  return typeof digest === "string" && /^[a-f0-9]{64}$/.test(digest) ? digest : undefined;
}

function responseApplicationRuntime(value: unknown): { state: "synced" | "pending" | "failed"; reason?: string } | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const result = (value as { result?: unknown }).result;
  if (!result || typeof result !== "object" || Array.isArray(result)) return undefined;
  const runtime = (result as { application_runtime?: unknown }).application_runtime;
  if (!runtime || typeof runtime !== "object" || Array.isArray(runtime)) return undefined;
  const state = (runtime as { state?: unknown }).state;
  if (state !== "synced" && state !== "pending" && state !== "failed") return undefined;
  const reason = (runtime as { reason?: unknown }).reason;
  return typeof reason === "string" ? { state, reason } : { state };
}

function saveFailureMessage(error: unknown): string {
  if (!(error instanceof BridgeError) || error.code !== "daemon_error") return "自动保存失败，已恢复上次生效的应用策略";
  if (error.daemonCode === "NH-CONFIG-CAPABILITY-UNAVAILABLE") return "自动保存失败：Android 应用目录尚未就绪，请稍后重试";
  if (error.daemonCode === "NH-CONFIG-CONFLICT") return "自动保存失败：配置已被其他操作修改，请重新加载后重试";
  return "自动保存失败，已恢复上次生效的应用策略";
}

async function drainSaveQueue(): Promise<void> {
  if (saving.value) return;
  while (saveRequested) {
    saveRequested = false;
    const active = uiStores.config.active.value;
    const digest = uiStores.config.baseDigest.value;
    if (!active || !digest || !dirty.value || missingSelection.value) return;

    const targetMode = mode.value;
    const targetSelected = new Set(selected.value);
    const document = buildApplicationPolicyDocument(active.document, targetMode, targetSelected);
    const mutation = buildApplicationPolicyMutation(active.document, targetMode, targetSelected);
    saving.value = true;
    operations.begin("app-policy", "applications");
    operations.update("app-policy", "running");
    try {
      const response = await uploadPrivatePayload(host, "config", "config-mutate", JSON.stringify({ expected_config_digest: digest, mutation }));
      const nextDigest = responseDigest(response) ?? digest;
      const applicationRuntime = responseApplicationRuntime(response);
      uiStores.config.load({
        ...active,
        observedConfigDigest: nextDigest,
        activeConfigDigest: nextDigest,
        candidateSequence: active.candidateSequence + 1,
        document,
        ...(applicationRuntime === undefined ? {} : { applicationRuntime }),
      });
      committedMode.value = targetMode;
      committedSelected.value = targetMode === "all" ? new Set() : targetSelected;
      operations.update("app-policy", "success", { message: applicationRuntime?.state === "pending" ? "应用策略已保存，等待 Android 应用目录同步" : "应用策略已自动保存" });
    } catch (error) {
      saveRequested = false;
      cancelScheduledSave();
      mode.value = committedMode.value;
      selected.value = new Set(committedSelected.value);
      operations.update("app-policy", "failure", { message: saveFailureMessage(error) });
    } finally {
      saving.value = false;
    }
  }
}

function cancelScheduledSave(): void {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = undefined;
  firstQueuedAt = 0;
}

function queuePolicySave(): void {
  const now = Date.now();
  if (firstQueuedAt === 0) firstQueuedAt = now;
  if (saveTimer) clearTimeout(saveTimer);
  const delay = Math.max(0, Math.min(400, 1_200 - (now - firstQueuedAt)));
  saveTimer = setTimeout(() => {
    saveTimer = undefined;
    firstQueuedAt = 0;
    saveRequested = true;
    void drainSaveQueue();
  }, delay);
}

onActivated(() => {
  if (packages.value.length === 0) { void load(); return; }
  if (dirty.value || saving.value) return;
  void validatedQuery(host, { id: "config.get" }, parseConfig).then((config) => {
    uiStores.config.load(config);
    applyPolicy(config.document);
  }).catch(() => undefined);
});
onBeforeUnmount(() => {
  cancelScheduledSave();
  if (!dirty.value || missingSelection.value) return;
  saveRequested = true;
  void drainSaveQueue();
});
</script>

<template>
  <section class="page applications-page">
    <div class="page-heading applications-heading"><h2>应用</h2><ApplicationSortDropdown :model-value="sort" :fields="availableSortFields" :selected-first="selectedFirst" :selected-count="selectedCount" @update:model-value="updateSort" @selected-first-change="updateSelectedFirst" /></div>
    <OperationBanner v-if="operations.byId['app-policy']" :phase="operations.byId['app-policy']!.phase" :message="operations.byId['app-policy']!.message ?? ''" @dismiss="operations.clear('app-policy')" />
    <PageState v-if="loading" :model="{ type: 'loading', title: '正在读取应用列表' }" />
    <PageState v-else-if="error" :model="{ type: 'error', title: '应用列表不可用', detail: error }" action-label="重试" @action="load" />
    <template v-else>
      <section class="application-mode-card">
        <div class="application-mode"><Segmented :model-value="mode" :options="modeOptions" @change="changeMode" /></div>
      </section>
      <Transition :name="transitionName" mode="out-in">
        <div v-if="mode === 'all'" key="all" class="application-global-spacer"></div>
        <section v-else :key="mode" class="application-selection-panel">
          <div class="filter-bar"><ApplicationSearch v-model="query" placeholder="搜索应用名称或包名" /><ApplicationCategoryDropdown v-model="category" :options="categoryOptions" /></div>
          <div class="application-selection-summary">
            <div><strong>已选应用</strong><span class="application-selected-count">{{ selectedCount }}</span><span class="application-selection-effect">· {{ selectionEffect }}</span><small v-if="missingSelection">至少选择一个应用</small></div>
            <div class="application-batch-actions"><Button size="s" variant="text" @click="selectVisible">全选</Button><Button size="s" variant="text" @click="clearVisible">清空</Button></div>
          </div>
          <PageState v-if="rows.length === 0" :model="{ type: 'empty', title: '没有匹配的应用' }" />
          <PullRefresh v-else v-model="refreshing" :disabled="loading || saving" @refresh="pullRefresh">
          <div class="application-list">
            <VirtualListViewport :items="rows" :get-item-key="(_index, app) => app.packageName" :estimate-size="78" :style="{ height: `min(58dvh, ${rows.length * 78}px)` }">
              <template #default="{ item: app }"><div class="app-row" :data-selected="app.selected"><ApplicationIcon :host="host" :app="app" /><div class="app-main"><div class="app-title"><strong>{{ app.appLabel || app.packageName }}</strong><Tag v-if="app.isSystem" size="s" variant="soft">系统</Tag></div><small>{{ app.packageName }}</small><em v-if="app.sharedCount > 1">共享 UID，同时影响 {{ app.sharedCount }} 个应用</em><em v-if="app.uid === 0">root UID 受保护</em></div><Switch size="s" :model-value="app.selected" :disabled="app.uid === 0" :aria-label="`代理 ${app.appLabel || app.packageName}`" @change="(value) => toggle(app, value)" /></div></template>
            </VirtualListViewport>
          </div>
          </PullRefresh>
        </section>
      </Transition>
    </template>
  </section>
</template>
