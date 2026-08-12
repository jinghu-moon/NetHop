<script setup lang="ts">
import { computed, h, nextTick, onActivated, ref } from "vue";
import { IconBolt, IconCopy, IconDotsVertical, IconRefresh, IconTrash, IconX } from "@tabler/icons-vue";
import { ActionSheet as TActionSheet, Button as TButton, PullDownRefresh as TPullDownRefresh } from "tdesign-mobile-vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/PageState.vue";
import { useHost } from "@/bridge/context";
import { runJson } from "@/bridge/command";
import { validatedQuery } from "@/model/client";
import { parseControlEnvelope, parseNodeDelayList, parseNodeList, parseNodeSelection, parseSubscriptionSnapshot, type NodeDto } from "@/model/dto";
import { uiStores } from "@/runtime/store";
import { createActionLock, createOperationStore } from "@/runtime/operation";
import { parseNodeSort, sortNodes, type NodeSort } from "@/model/node-view";
import { useUiPreference } from "@/runtime/storage";
import { useBackDismiss } from "@/shell/useBackDispatcher";

type PageAction = "refresh" | "sort-default" | "sort-name" | "sort-latency" | "clear-delay" | "export" | "exclude";
type NodeListItem = { readonly kind: "heading"; readonly id: string; readonly label: string; readonly count: number } | { readonly kind: "row"; readonly id: string; readonly nodes: readonly NodeDto[] };

const host = useHost();
const loading = ref(false);
const error = ref("");
const pendingExclude = ref<NodeDto>();
const actionSheetOpen = ref(false);
const queuedAction = ref<PageAction>();
const lock = createActionLock();
const operations = createOperationStore();
const refreshing = ref(false);
const sourceNames = ref<Readonly<Record<string, string>>>({});
const sortPreference = useUiPreference("node-sort", "default");
const nodeSort = computed(() => parseNodeSort(sortPreference.value));

const allNodes = computed(() => uiStores.runtime.nodeOrder.value.map((id) => uiStores.runtime.nodesById.value[id]).filter((node): node is NodeDto => Boolean(node)));
const selection = computed(() => uiStores.runtime.selection.value);
const automatic = computed(() => selection.value?.intent.mode === "auto");
const nodeItems = computed<readonly NodeListItem[]>(() => {
  const groups = new Map<string, NodeDto[]>();
  for (const node of allNodes.value) {
    const sourceId = node.sourceIds[0] ?? "other";
    const group = groups.get(sourceId) ?? [];
    group.push(node);
    groups.set(sourceId, group);
  }
  const items: NodeListItem[] = [];
  for (const [sourceId, group] of groups) {
    items.push({ kind: "heading", id: `heading:${sourceId}`, label: sourceNames.value[sourceId] ?? "其他来源", count: group.length });
    const sorted = sortNodes(group, nodeSort.value);
    for (let index = 0; index < sorted.length; index += 2) items.push({ kind: "row", id: `row:${sourceId}:${index}`, nodes: sorted.slice(index, index + 2) });
  }
  return items;
});
const selectedNode = computed(() => allNodes.value.find((node) => node.isRequested) ?? allNodes.value.find((node) => node.isActive));
const testOperation = computed(() => operations.byId["node-test-all"]);
const testingAll = computed(() => testOperation.value?.phase === "accepted" || testOperation.value?.phase === "running");
const actionItems = computed(() => [
  { label: "刷新节点列表", icon: () => h(IconRefresh, { size: 20 }) },
  { label: "默认排序", disabled: nodeSort.value === "default" },
  { label: "按名称排序", disabled: nodeSort.value === "name" },
  { label: "按延迟排序", disabled: nodeSort.value === "latency" },
  { label: "清除测速结果", icon: () => h(IconX, { size: 20 }), disabled: allNodes.value.every((node) => node.latencyMs === undefined) },
  { label: "导出当前节点", icon: () => h(IconCopy, { size: 20 }), disabled: !selectedNode.value },
  { label: "排除当前节点", icon: () => h(IconTrash, { size: 20 }), color: "var(--nh-danger)", disabled: !selectedNode.value },
]);

async function load(): Promise<void> {
  if (loading.value) return;
  loading.value = true; error.value = "";
  try {
    const [snapshot, sources] = await Promise.all([
      validatedQuery(host, { id: "node.list", limit: 128 }, parseNodeList),
      validatedQuery(host, { id: "subscription.mode.get" }, parseSubscriptionSnapshot),
    ]);
    uiStores.runtime.loadNodeSnapshot(snapshot);
    sourceNames.value = Object.fromEntries(sources.sources.map((source) => [source.id, source.name]));
  } catch { error.value = "节点列表加载失败"; }
  finally { loading.value = false; }
}
async function pullRefresh(): Promise<void> { refreshing.value = true; try { await load(); } finally { refreshing.value = false; } }
async function testAllNodes(): Promise<void> {
  if (testingAll.value || allNodes.value.length === 0) return;
  const operationId = "node-test-all";
  operations.begin(operationId, "node-delay");
  operations.update(operationId, "running", { message: "正在测试全部节点" });
  try {
    const response = await runJson(host, { id: "node.test-all" });
    const results = parseControlEnvelope(response.response, parseNodeDelayList).result;
    const delays = new Map(results.map((result) => [result.id, result.latencyMs]));
    let updated = 0;
    for (const node of allNodes.value) {
      const latencyMs = delays.get(node.id);
      if (latencyMs === undefined) {
        const { latencyMs: _previousLatency, ...withoutLatency } = node;
        uiStores.runtime.upsertNode(withoutLatency);
      } else {
        uiStores.runtime.upsertNode({ ...node, latencyMs });
        updated += 1;
      }
    }
    operations.update(operationId, "success", { message: `测速完成：成功 ${updated} / ${allNodes.value.length}` });
  } catch {
    operations.update(operationId, "failure", { message: "节点测速失败" });
  }
}
async function selectNode(node: NodeDto): Promise<void> {
  await lock.run("node-select", async () => {
    const next = await validatedQuery(host, { id: "node.select.manual", nodeId: node.id }, parseNodeSelection);
    uiStores.runtime.setSelection(next);
    await load();
  });
}
async function selectAuto(): Promise<void> {
  if (automatic.value) return;
  await lock.run("node-select", async () => {
    const next = await validatedQuery(host, { id: "node.select.auto" }, parseNodeSelection);
    uiStores.runtime.setSelection(next);
    await load();
  });
}
async function excludeNode(): Promise<void> {
  const node = pendingExclude.value; if (!node) return;
  await runJson(host, { id: "node.remove", nodeId: node.id, expectedDigest: uiStores.config.baseDigest.value ?? "0".repeat(64) });
  pendingExclude.value = undefined; await load();
}
async function exportNode(node: NodeDto): Promise<void> {
  const response = await runJson(host, { id: "node.export", nodeId: node.id });
  const envelope = response.response as { result?: unknown };
  if (envelope.result !== undefined) await navigator.clipboard.writeText(typeof envelope.result === "string" ? envelope.result : JSON.stringify(envelope.result));
}
function clearDelays(): void {
  for (const node of allNodes.value) {
    const { latencyMs: _latencyMs, ...withoutLatency } = node;
    uiStores.runtime.upsertNode(withoutLatency);
  }
}
function selectAction(_selected: unknown, actionIndex: number): void {
  queuedAction.value = (["refresh", "sort-default", "sort-name", "sort-latency", "clear-delay", "export", "exclude"] as const)[actionIndex];
}
function finishActionSheet(): void {
  const action = queuedAction.value;
  queuedAction.value = undefined;
  if (!action) return;
  void nextTick().then(async () => {
    if (action === "refresh") await load();
    else if (action.startsWith("sort-")) sortPreference.value = action.slice(5) as NodeSort;
    else if (action === "clear-delay") clearDelays();
    else if (action === "export" && selectedNode.value) await exportNode(selectedNode.value);
    else if (action === "exclude" && selectedNode.value) pendingExclude.value = selectedNode.value;
  });
}
onActivated(() => { void load(); });
useBackDismiss(() => actionSheetOpen.value, () => { actionSheetOpen.value = false; });
</script>

<template>
  <section class="page nodes-page">
    <div class="page-heading">
      <div><span class="eyebrow">OUTBOUNDS</span><h2>节点</h2><p>{{ allNodes.length }} 个可用节点</p></div>
      <div class="heading-actions">
        <TButton size="small" shape="square" theme="primary" title="测试全部节点" :loading="testingAll" :disabled="testingAll || allNodes.length === 0" @click="testAllNodes"><IconBolt :size="18" /></TButton>
        <TButton size="small" shape="square" variant="outline" theme="default" title="更多操作" @click="actionSheetOpen = true"><IconDotsVertical :size="19" /></TButton>
      </div>
    </div>
    <OperationBanner v-if="testOperation" :phase="testOperation.phase" :message="testOperation.message ?? ''" @dismiss="operations.clear('node-test-all')" />
    <PageState v-if="loading" kind="loading" title="正在加载节点" />
    <PageState v-else-if="error" kind="error" title="节点加载失败" :detail="error" action-label="重试" @action="load" />
    <PageState v-else-if="allNodes.length === 0" kind="empty" title="没有可用节点" />
    <TPullDownRefresh v-else v-model="refreshing" :disabled="loading" @refresh="pullRefresh">
    <button type="button" class="node-auto-control" :data-selected="automatic" @click="selectAuto">
      <span><strong>自动优选</strong><small>{{ automatic ? "由 sing-box URLTest 自动选择" : "点击恢复自动选择" }}</small></span>
      <span v-if="automatic && selection?.activeNodeId" class="node-auto-active">{{ uiStores.runtime.nodesById.value[selection.activeNodeId]?.name ?? "状态同步中" }}</span>
    </button>
    <VirtualListViewport :items="nodeItems" :get-item-key="(_index, item) => item.id" :estimate-size="82">
      <template #default="{ item }">
        <div v-if="item.kind === 'heading'" class="node-source-heading"><strong>{{ item.label }}</strong><span>{{ item.count }}</span></div>
        <div v-else class="node-grid-row">
          <article v-for="node in item.nodes" :key="node.id" class="node-card" :data-requested="node.isRequested" :data-active="node.isActive" @click="!node.isRequested && selectNode(node)">
            <div class="node-main"><strong :title="node.name">{{ node.name }}</strong><span class="node-protocol">{{ node.protocol }}</span></div>
            <span class="node-latency" :data-ready="node.latencyMs !== undefined">{{ node.latencyMs === undefined ? '--' : `${node.latencyMs} ms` }}</span>
          </article>
        </div>
      </template>
    </VirtualListViewport>
    </TPullDownRefresh>
    <TActionSheet v-model="actionSheetOpen" class="node-actions-sheet" theme="list" align="left" show-cancel cancel-text="取消" description="节点操作" :items="actionItems" @selected="selectAction" @close="finishActionSheet" />
    <ConfirmDialog :visible="Boolean(pendingExclude)" title="排除节点" :description="`排除“${pendingExclude?.name ?? ''}”后，后续订阅更新也不会重新加入该节点。`" confirm-label="排除" @update:visible="(value) => { if (!value) pendingExclude = undefined; }" @confirm="excludeNode" />
  </section>
</template>
