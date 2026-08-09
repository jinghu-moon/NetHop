<script setup lang="ts">
import { computed, h, onActivated, onBeforeUnmount, ref } from "vue";
import { useDebounce } from "@vueuse/core";
import { IconDotsVertical, IconSortAscending, IconSortDescending } from "@tabler/icons-vue";
import { ActionSheet as TActionSheet, Button as TButton, PullDownRefresh as TPullDownRefresh, Switch as TSwitch, Tag as TTag } from "tdesign-mobile-vue";
import ApplicationCategoryDropdown from "@/components/applications/ApplicationCategoryDropdown.vue";
import ApplicationSearch from "@/components/applications/ApplicationSearch.vue";
import SegmentedControl from "@/components/SegmentedControl.vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";
import PageState from "@/components/PageState.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { readPackages } from "@/bridge/package-adapter";
import { validatedQuery } from "@/model/client";
import { parseConfig } from "@/model/dto";
import { buildApplicationPolicyDocument, buildApplicationPolicyMutation, readApplicationPolicy, type ApplicationMode } from "@/model/application-policy";
import { uiStores } from "@/runtime/store";
import { createOperationStore } from "@/runtime/operation";
import { SearchIndex } from "@/runtime/search-index";
import type { PackageInfo } from "@/bridge/host";
import { hasApplicationSortData, parseApplicationSort, prioritizeSelected, serializeApplicationSort, sortApplications, type ApplicationSort, type ApplicationSortField, type ApplicationSortDirection } from "@/model/application-sort";
import { useUiPreference } from "@/runtime/storage";
import { useBackDismiss } from "@/shell/useBackDispatcher";

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
const sortOptions = computed(() => [{ value: "selected-first", label: selectedFirst.value ? "取消已选优先" : "已选优先", icon: () => h(IconSortAscending, { size: 20 }) }, ...sortFields.flatMap((item) => ([
  { value: `${item.field}:asc`, label: `${item.label} · 升序`, icon: () => h(IconSortAscending, { size: 20 }), disabled: !hasApplicationSortData(packages.value, item.field) },
  { value: `${item.field}:desc`, label: `${item.label} · 降序`, icon: () => h(IconSortDescending, { size: 20 }), disabled: !hasApplicationSortData(packages.value, item.field) },
]))]);
const sortSheetOpen = ref(false);
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
      Promise.resolve().then(() => readPackages(host, "all").slice(0, 2_000)),
    ]);
    uiStores.config.load(config);
    applyPolicy(config.document);
    packages.value = packageList;
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
function selectSort(_selected: unknown, index: number): void {
  const option = sortOptions.value[index];
  if (!option || ("disabled" in option && option.disabled)) return;
  if (option.value === "selected-first") {
    selectedFirstPreference.value = selectedFirst.value ? "false" : "true";
    return;
  }
  const [field, direction] = option.value.split(":") as [ApplicationSortField, ApplicationSortDirection];
  sortPreference.value = serializeApplicationSort({ field, direction });
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
      uiStores.config.load({
        ...active,
        observedConfigDigest: nextDigest,
        activeConfigDigest: nextDigest,
        candidateSequence: active.candidateSequence + 1,
        document,
      });
      committedMode.value = targetMode;
      committedSelected.value = targetMode === "all" ? new Set() : targetSelected;
      operations.update("app-policy", "success", { message: "应用策略已自动保存" });
    } catch {
      saveRequested = false;
      cancelScheduledSave();
      mode.value = committedMode.value;
      selected.value = new Set(committedSelected.value);
      operations.update("app-policy", "failure", { message: "自动保存失败，已恢复上次生效的应用策略" });
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
useBackDismiss(() => sortSheetOpen.value, () => { sortSheetOpen.value = false; });
onBeforeUnmount(() => {
  cancelScheduledSave();
  if (!dirty.value || missingSelection.value) return;
  saveRequested = true;
  void drainSaveQueue();
});
</script>

<template>
  <section class="page applications-page">
    <div class="page-heading applications-heading"><h2>应用</h2><TButton size="small" shape="square" variant="outline" theme="default" title="排序方式" @click="sortSheetOpen = true"><IconDotsVertical :size="19" /></TButton></div>
    <OperationBanner v-if="operations.byId['app-policy']" :phase="operations.byId['app-policy']!.phase" :message="operations.byId['app-policy']!.message ?? ''" @dismiss="operations.clear('app-policy')" />
    <PageState v-if="loading" kind="loading" title="正在读取应用列表" />
    <PageState v-else-if="error" kind="error" title="应用列表不可用" :detail="error" action-label="重试" @action="load" />
    <template v-else>
      <section class="application-mode-card">
        <div class="application-mode"><SegmentedControl :model-value="mode" :options="modeOptions" @change="changeMode" /></div>
      </section>
      <Transition :name="transitionName" mode="out-in">
        <div v-if="mode === 'all'" key="all" class="application-global-spacer"></div>
        <section v-else :key="mode" class="application-selection-panel">
          <div class="filter-bar"><ApplicationSearch v-model="query" placeholder="搜索应用名称或包名" /><ApplicationCategoryDropdown v-model="category" :options="categoryOptions" /></div>
          <div class="application-selection-summary">
            <div><strong>已选应用</strong><span class="application-selected-count">{{ selectedCount }}</span><span class="application-selection-effect">· {{ selectionEffect }}</span><small v-if="missingSelection">至少选择一个应用</small></div>
            <div class="application-batch-actions"><TButton size="small" variant="text" @click="selectVisible">全选</TButton><TButton size="small" variant="text" @click="clearVisible">清空</TButton></div>
          </div>
          <PageState v-if="rows.length === 0" kind="empty" title="没有匹配的应用" />
          <TPullDownRefresh v-else v-model="refreshing" :disabled="loading || saving" @refresh="pullRefresh">
          <div class="application-list">
            <VirtualListViewport :items="rows" :get-item-key="(_index, app) => app.packageName" :estimate-size="78" :style="{ height: `min(58dvh, ${rows.length * 78}px)` }">
              <template #default="{ item: app }"><div class="app-row" :data-selected="app.selected"><div class="app-icon"><span>{{ (app.appLabel || app.packageName).slice(0, 1).toUpperCase() }}</span><img v-if="host.capability.kind !== 'browser'" :src="`ksu://icon/${encodeURIComponent(app.packageName)}`" @error="($event.target as HTMLImageElement).hidden = true" /></div><div class="app-main"><div class="app-title"><strong>{{ app.appLabel || app.packageName }}</strong><TTag v-if="app.isSystem" size="small" variant="light">系统</TTag></div><small>{{ app.packageName }}</small><em v-if="app.sharedCount > 1">共享 UID，同时影响 {{ app.sharedCount }} 个应用</em><em v-if="app.uid === 0">root UID 受保护</em></div><TSwitch size="small" :value="app.selected" :disabled="app.uid === 0" @change="(value) => toggle(app, value)" /></div></template>
            </VirtualListViewport>
          </div>
          </TPullDownRefresh>
        </section>
      </Transition>
    </template>
    <TActionSheet v-model="sortSheetOpen" class="application-sort-sheet" theme="list" align="left" show-cancel cancel-text="取消" description="应用排序" :items="sortOptions" @selected="selectSort" />
  </section>
</template>
