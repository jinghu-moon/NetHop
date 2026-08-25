<script setup lang="ts">
import { computed, onActivated, ref } from "vue";
import { IconBolt } from "@tabler/icons-vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";
import ActiveNodeSummary from "@/components/nodes/ActiveNodeSummary.vue";
import NodeActionsDropdown from "@/components/nodes/NodeActionsDropdown.vue";
import NodeCard from "@/components/nodes/NodeCard.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/ui/feedback/PageState.vue";
import { useHost } from "@/bridge/context";
import { runJson } from "@/bridge/command";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { validatedQuery } from "@/model/client";
import { parseControlEnvelope, parseNodeBenchmarkResult, parseNodeList, parseNodeSelection, parseSubscriptionSnapshot, type NodeDto } from "@/model/dto";
import { uiStores } from "@/runtime/store";
import { createActionLock, createOperationStore } from "@/runtime/operation";
import { activeNodeId, activeNodeView, parseNodeSort, sortNodes, type NodeSort } from "@/model/node-view";
import { buildNodeOverridePayload, parseNodeOutbound, parseNodeOverride, serializeNodeOutbound } from "@/model/node-editor";
import { useUiPreference } from "@/runtime/storage";
import Input from "@/components/ui/primitives/Input.vue";
import Textarea from "@/components/ui/primitives/Textarea.vue";
import Field from "@/components/ui/form/Field.vue";
import Button from "@/components/ui/primitives/Button.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import Popup from "@/components/ui/overlay/Popup.vue";
import Dialog from "@/components/ui/overlay/Dialog.vue";
import PullRefresh from "@/components/ui/feedback/PullRefresh.vue";

type NodeListItem = { readonly kind: "heading"; readonly id: string; readonly label: string; readonly count: number } | { readonly kind: "row"; readonly id: string; readonly nodes: readonly NodeDto[] };

const host = useHost();
const loading = ref(false);
const error = ref("");
const pendingExclude = ref<NodeDto>();
const editorOpen = ref(false);
const editorLoading = ref(false);
const editorSaving = ref(false);
const editorNode = ref<NodeDto>();
const editorName = ref("");
const editorOutbound = ref("");
const editorError = ref("");
const editorOverridden = ref(false);
const lock = createActionLock();
const operations = createOperationStore();
const refreshing = ref(false);
const sourceNames = ref<Readonly<Record<string, string>>>({});
const probeStates = computed(() => uiStores.runtime.nodeProbeStates.value);
const fastSelection = computed(() => uiStores.runtime.nodeBenchmarkFastSelection.value);
const backgroundRemaining = computed(() => {
  const state = fastSelection.value;
  return state && state.state !== "pending" ? Math.max(0, state.candidateCount - state.completed) : 0;
});
const sortPreference = useUiPreference("node-sort", "default");
const nodeSort = computed(() => parseNodeSort(sortPreference.value));

const allNodes = computed(() => uiStores.runtime.nodeOrder.value.map((id) => uiStores.runtime.nodesById.value[id]).filter((node): node is NodeDto => Boolean(node)));
const selection = computed(() => uiStores.runtime.selection.value);
const automatic = computed(() => selection.value?.intent.mode === "auto");
const activeId = computed(() => activeNodeId(selection.value));
const serviceRunning = computed(() => {
  const state = uiStores.session.status.value?.state;
  return state === undefined || state === "running_tproxy" || state === "running_tun";
});
const activeSummary = computed(() => activeNodeView(selection.value, uiStores.runtime.nodesById.value, sourceNames.value, serviceRunning.value));
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
const hasDelayResults = computed(() => allNodes.value.some((node) => node.latencyMs !== undefined));
const testOperation = computed(() => operations.byId["node-test-all"]);
const testingAll = computed(() => testOperation.value?.phase === "accepted" || testOperation.value?.phase === "running");

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
  uiStores.runtime.beginNodeBenchmark(allNodes.value.map((node) => node.id));
  try {
    const response = await runJson(host, { id: "node.test-all" });
    const result = parseControlEnvelope(response.response, parseNodeBenchmarkResult).result;
    uiStores.runtime.finishNodeBenchmark(result.report.nodes, result.fastSelection);
    if (result.selection) uiStores.runtime.setSelection(result.selection);
    operations.update(operationId, "success", { message: `测速完成：成功 ${result.report.succeeded} / ${result.report.tested}` });
  } catch {
    uiStores.runtime.clearNodeProbeStates();
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
async function openNodeEditor(): Promise<void> {
  const node = selectedNode.value; if (!node) return;
  editorNode.value = node; editorOpen.value = true; editorLoading.value = true; editorError.value = "";
  try {
    const value = await validatedQuery(host, { id: "node.override.get", nodeId: node.id }, parseNodeOverride);
    editorName.value = value.displayName;
    editorOutbound.value = serializeNodeOutbound(value.outbound);
    editorOverridden.value = value.overridden;
  } catch { editorError.value = "节点编辑数据加载失败"; }
  finally { editorLoading.value = false; }
}
async function saveNodeEditor(): Promise<void> {
  const node = editorNode.value; if (!node || editorSaving.value) return;
  editorError.value = "";
  let outbound: Readonly<Record<string, unknown>>;
  try { outbound = parseNodeOutbound(editorOutbound.value); }
  catch (cause) { editorError.value = cause instanceof Error ? cause.message : "节点 outbound 无效"; return; }
  editorSaving.value = true;
  try {
    await uploadPrivatePayload(host, "node", "node-override-apply", buildNodeOverridePayload(node.id, editorName.value, outbound));
    editorOpen.value = false;
    await load();
  } catch { editorError.value = "节点保存失败，当前运行配置保持不变"; }
  finally { editorSaving.value = false; }
}
async function restoreNodeEditor(): Promise<void> {
  const node = editorNode.value; if (!node || !editorOverridden.value || editorSaving.value) return;
  editorSaving.value = true; editorError.value = "";
  try {
    await runJson(host, { id: "node.override.remove", nodeId: node.id });
    editorOpen.value = false;
    await load();
  } catch { editorError.value = "恢复订阅原值失败"; }
  finally { editorSaving.value = false; }
}
function closeNodeEditor(): void {
  if (editorSaving.value) return;
  editorOpen.value = false; editorNode.value = undefined; editorError.value = "";
}
function clearDelays(): void {
  uiStores.runtime.clearNodeProbeStates();
  for (const node of allNodes.value) {
    const { latencyMs: _latencyMs, ...withoutLatency } = node;
    uiStores.runtime.upsertNode(withoutLatency);
  }
}
function changeSort(value: NodeSort): void {
  sortPreference.value = value;
}
function exportSelectedNode(): void {
  if (selectedNode.value) void exportNode(selectedNode.value);
}
function requestExcludeSelectedNode(): void {
  pendingExclude.value = selectedNode.value;
}
onActivated(() => { void load(); });
</script>

<template>
  <section class="page nodes-page">
    <div class="page-heading">
      <div><span class="eyebrow">OUTBOUNDS</span><h2>节点</h2><p>{{ allNodes.length }} 个可用节点</p></div>
      <div class="heading-actions">
        <IconButton size="s" variant="primary" aria-label="测试全部节点" title="测试全部节点" :loading="testingAll" :disabled="testingAll || allNodes.length === 0" @click="testAllNodes"><IconBolt :size="18" /></IconButton>
        <NodeActionsDropdown
          :sort="nodeSort"
          :has-delay-results="hasDelayResults"
          :has-selected-node="Boolean(selectedNode)"
          @refresh="load"
          @sort-change="changeSort"
          @clear-delays="clearDelays"
          @export="exportSelectedNode"
          @edit="openNodeEditor"
          @exclude="requestExcludeSelectedNode"
        />
      </div>
    </div>
    <OperationBanner v-if="testOperation" :phase="testOperation.phase" :message="testOperation.message ?? ''" @dismiss="operations.clear('node-test-all')" />
    <PageState v-if="loading" :model="{ type: 'loading', title: '正在加载节点' }" />
    <PageState v-else-if="error" :model="{ type: 'error', title: '节点加载失败', detail: error }" action-label="重试" @action="load" />
    <PageState v-else-if="allNodes.length === 0" :model="{ type: 'empty', title: '没有可用节点' }" />
    <PullRefresh v-else v-model="refreshing" :disabled="loading" @refresh="pullRefresh">
    <ActiveNodeSummary :value="activeSummary" />
    <div v-if="fastSelection?.state === 'switched' && backgroundRemaining > 0" class="node-background-benchmark">
      已完成快速选优，剩余 {{ backgroundRemaining }} 个节点测速中
    </div>
    <button type="button" class="node-auto-control" :data-selected="automatic" @click="selectAuto">
      <span><strong>自动优选</strong><small>{{ automatic ? "由 NetHop 定期测速并自动选择" : "点击恢复自动选择" }}</small></span>
      <span v-if="automatic && activeId" class="node-auto-active">{{ uiStores.runtime.nodesById.value[activeId]?.name ?? "状态同步中" }}</span>
    </button>
    <VirtualListViewport :items="nodeItems" :get-item-key="(_index, item) => item.id" :estimate-size="82">
      <template #default="{ item }">
        <div v-if="item.kind === 'heading'" class="node-source-heading"><strong>{{ item.label }}</strong><span>{{ item.count }}</span></div>
        <div v-else class="node-grid-row">
          <NodeCard v-for="node in item.nodes" :key="node.id" :node="node" :probe-state="probeStates[node.id]" @select="selectNode" />
        </div>
      </template>
    </VirtualListViewport>
    </PullRefresh>
    <Popup v-model="editorOpen" @visible-change="(visible) => { if (!visible) closeNodeEditor(); }">
      <div class="subscription-editor node-editor">
        <div class="editor-heading"><h3>编辑节点</h3><span>保存后由 daemon 校验并事务化发布</span></div>
        <PageState v-if="editorLoading" :model="{ type: 'loading', title: '正在读取节点' }" />
        <template v-else>
          <Field label="显示名称" required><Input v-model="editorName" variant="outline" :maxlength="128" /></Field>
          <Field label="终端 outbound JSON" required><Textarea v-model="editorOutbound" variant="outline" :maxlength="65536" :min-rows="12" :max-rows="24" /></Field>
          <span v-if="editorError" class="form-error">{{ editorError }}</span>
          <div class="editor-actions">
            <Button v-if="editorOverridden" variant="danger" :disabled="editorSaving" @click="restoreNodeEditor">恢复订阅原值</Button>
            <Button variant="outline" :disabled="editorSaving" @click="closeNodeEditor">取消</Button>
            <Button variant="primary" :loading="editorSaving" @click="saveNodeEditor">保存</Button>
          </div>
        </template>
      </div>
    </Popup>
    <Dialog :model-value="Boolean(pendingExclude)" aria-label="排除节点" @update:model-value="(value) => { if (!value) pendingExclude = undefined; }">
      <template #title>排除节点</template>
      <p>排除“{{ pendingExclude?.name ?? '' }}”后，后续订阅更新也不会重新加入该节点。</p>
      <template #actions>
        <Button variant="outline" @click="pendingExclude = undefined">取消</Button>
        <Button variant="danger" @click="excludeNode">排除</Button>
      </template>
    </Dialog>
  </section>
</template>
