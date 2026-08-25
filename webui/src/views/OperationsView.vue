<script setup lang="ts">
import { computed, onActivated, onDeactivated, ref } from "vue";
import { useIntervalFn } from "@vueuse/core";
import { IconArchive, IconArrowDown, IconArrowUp, IconBolt, IconBug, IconRefresh, IconTrash, IconX } from "@tabler/icons-vue";
import PageState from "@/components/ui/feedback/PageState.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";
import { useHost } from "@/bridge/context";
import { runJson } from "@/bridge/command";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { validatedQuery } from "@/model/client";
import { parseLogs, parseOperational, parseRuntimeMetrics, type LogChannelDto, type LogEntryDto, type RuntimeMetricsDto } from "@/model/dto";
import { createOperationStore } from "@/runtime/operation";
import { uiStores } from "@/runtime/store";
import Textarea from "@/components/ui/primitives/Textarea.vue";
import Button from "@/components/ui/primitives/Button.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import Tabs from "@/components/ui/navigation/Tabs.vue";
import Dialog from "@/components/ui/overlay/Dialog.vue";

interface ConnectionRow { readonly id: string; readonly network: string; readonly source: string; readonly destination: string }
interface VirtualListHandle { scrollToStart(): void; scrollToEnd(): void }

const host = useHost();
const tab = ref("connections");
const loading = ref(false);
const connections = ref<readonly ConnectionRow[]>([]);
const logs = ref<readonly LogEntryDto[]>([]);
const metrics = ref<RuntimeMetricsDto>();
const topology = ref<Readonly<Record<string, unknown>>>();
const ruleset = ref<Readonly<Record<string, unknown>>>();
const version = ref<Readonly<Record<string, unknown>>>();
const diagnostic = ref<Readonly<Record<string, unknown>>>();
const restoreText = ref("");
const confirmAction = ref<"connections" | "logs" | undefined>();
const logChannel = ref<LogChannelDto>("service");
const logView = ref<"structured" | "raw">("structured");
const logViewport = ref<VirtualListHandle>();
const operations = createOperationStore();
const metricsPolling = useIntervalFn(() => { if (tab.value === "system") void loadMetrics(); }, 5_000, { immediate: false });

const channelOptions = [{ value: "service", label: "服务" }, { value: "subscription", label: "订阅" }, { value: "core", label: "内核" }];
const viewOptions = [{ value: "structured", label: "结构化" }, { value: "raw", label: "原始" }];
const operationTabs = [{ value: "connections", label: "连接" }, { value: "logs", label: "日志" }, { value: "system", label: "系统" }, { value: "backup", label: "备份" }] as const;

function objects(value: Readonly<Record<string, unknown>>, keys: readonly string[]): readonly Readonly<Record<string, unknown>>[] {
  for (const key of keys) {
    const candidate = value[key];
    if (Array.isArray(candidate)) return candidate.filter((item): item is Readonly<Record<string, unknown>> => Boolean(item) && typeof item === "object" && !Array.isArray(item)).slice(0, 128);
  }
  return [];
}

function bytes(value: number | undefined): string {
  if (value === undefined) return "--";
  const units = ["B", "KiB", "MiB", "GiB"];
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; }
  return `${size >= 10 || unit === 0 ? size.toFixed(0) : size.toFixed(1)} ${units[unit]}`;
}

function duration(seconds: number | undefined): string {
  if (seconds === undefined) return "--";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return days > 0 ? `${days} 天 ${hours} 小时` : `${hours} 小时 ${minutes} 分钟`;
}

const systemRows = computed(() => [
  { label: "运行状态", value: metrics.value?.runtimeState ?? "未知" },
  { label: "核心 CPU", value: metrics.value?.core?.cpuPercent === undefined ? "--" : `${metrics.value.core.cpuPercent.toFixed(1)}%` },
  { label: "核心内存", value: bytes(metrics.value?.core?.memoryRssBytes) },
  { label: "运行时长", value: duration(metrics.value?.uptimeSeconds) },
  { label: "累计上传", value: bytes(metrics.value?.uploadBytes) },
  { label: "累计下载", value: bytes(metrics.value?.downloadBytes) },
  { label: "出口接口", value: metrics.value?.interface ?? "--" },
  { label: "本地地址", value: metrics.value?.localAddress ?? "--" },
  { label: "公网出口", value: metrics.value?.publicIp ?? "未探测" },
  { label: "规则集", value: String(ruleset.value?.state ?? ruleset.value?.status ?? "未知") },
  { label: "核心版本", value: String(version.value?.current_version ?? version.value?.current ?? "未知") },
]);

async function loadConnections(): Promise<void> {
  const result = await validatedQuery(host, { id: "connections.get", limit: 128 }, (value) => parseOperational(value, "connections"));
  connections.value = objects(result, ["connections", "items"]).map((item, index) => ({
    id: typeof item.id === "string" ? item.id : `connection-${index}`,
    network: typeof item.network === "string" ? item.network : "--",
    source: typeof item.source === "string" ? item.source : "--",
    destination: typeof item.destination === "string" ? item.destination : typeof item.target === "string" ? item.target : "--",
  }));
}

async function loadLogs(): Promise<void> {
  logs.value = await validatedQuery(host, { id: "logs.get", channel: logChannel.value, limit: 128 }, parseLogs);
}

async function loadMetrics(): Promise<void> {
  metrics.value = await validatedQuery(host, { id: "metrics.get" }, parseRuntimeMetrics);
}

async function loadSystem(): Promise<void> {
  const [topologyResult, rulesetResult, versionResult] = await Promise.all([
    validatedQuery(host, { id: "topology.get" }, (value) => parseOperational(value, "topology")),
    validatedQuery(host, { id: "ruleset.status" }, (value) => parseOperational(value, "ruleset")),
    validatedQuery(host, { id: "core.version-check" }, (value) => parseOperational(value, "version")),
  ]);
  topology.value = topologyResult;
  ruleset.value = rulesetResult;
  version.value = versionResult;
  await loadMetrics();
}

async function refresh(): Promise<void> {
  loading.value = true;
  try { await Promise.all([loadConnections(), loadLogs(), loadSystem()]); }
  finally { loading.value = false; }
}

function changeLogChannel(context: { value: string | number }): void {
  if (context.value !== "service" && context.value !== "subscription" && context.value !== "core") return;
  logChannel.value = context.value;
  void loadLogs();
}

function changeLogView(context: { value: string | number }): void {
  if (context.value === "structured" || context.value === "raw") logView.value = context.value;
}

async function closeConnection(id: string): Promise<void> { await runJson(host, { id: "connection.close", connectionId: id }); await loadConnections(); }
async function runConfirmed(): Promise<void> {
  const action = confirmAction.value;
  confirmAction.value = undefined;
  if (action === "connections") { await runJson(host, { id: "connections.close-all" }); await loadConnections(); }
  if (action === "logs") { await runJson(host, { id: "logs.clear" }); await loadLogs(); }
}
async function updateRuleset(): Promise<void> { operations.begin("ruleset", "ruleset"); operations.update("ruleset", "running"); try { await runJson(host, { id: "ruleset.update", wait: true }); await loadSystem(); operations.update("ruleset", "success", { message: "规则集更新完成" }); } catch { operations.update("ruleset", "failure", { message: "规则集更新失败" }); } }
async function createDiagnostic(): Promise<void> { operations.begin("diagnostic", "diagnostic"); operations.update("diagnostic", "running"); try { diagnostic.value = await validatedQuery(host, { id: "diagnostics.bundle" }, (value) => parseOperational(value, "diagnostics")); operations.update("diagnostic", "success", { message: "诊断包已生成" }); } catch { operations.update("diagnostic", "failure", { message: "诊断包生成失败" }); } }
async function exportBackup(): Promise<void> { operations.begin("backup", "backup"); operations.update("backup", "running"); try { await runJson(host, { id: "backup.export" }); host.toast("备份已写入 NetHop 私有备份目录"); operations.update("backup", "success", { message: "配置备份已导出" }); } catch { operations.update("backup", "failure", { message: "导出失败；目标文件可能已经存在" }); } }
async function restoreBackup(): Promise<void> {
  operations.begin("restore", "backup"); operations.update("restore", "running");
  try { const envelope = JSON.parse(restoreText.value) as { document?: unknown }; if (!envelope.document || typeof envelope.document !== "object" || Array.isArray(envelope.document)) throw new Error("invalid backup"); await uploadPrivatePayload(host, "backup", "backup-restore", JSON.stringify({ expected_config_digest: uiStores.config.baseDigest.value ?? "0".repeat(64), document: envelope.document })); restoreText.value = ""; operations.update("restore", "success", { message: "配置已恢复" }); }
  catch { operations.update("restore", "failure", { message: "备份无效或恢复失败" }); }
}

onActivated(() => {
  void refresh();
  metricsPolling.resume();
});
onDeactivated(metricsPolling.pause);
</script>

<template>
  <section class="page operations-page">
    <div class="page-heading"><div><span class="eyebrow">OPERATIONS</span><h2>运维</h2><p>连接、日志与运行资源</p></div><IconButton variant="outline" aria-label="刷新" @click="refresh"><IconRefresh :size="18" /></IconButton></div>
    <PageState v-if="loading" :model="{ type: 'loading', title: '正在读取运行状态' }" />
    <template v-else>
      <OperationBanner v-for="operation in Object.values(operations.byId)" :key="operation.id" :phase="operation.phase" :message="operation.message ?? ''" />
      <Tabs v-model="tab" :items="operationTabs" />
        <section v-if="tab === 'connections'" class="operation-tab-panel">
          <div class="section-heading"><h3>活动连接</h3><Button size="s" variant="danger" @click="confirmAction = 'connections'">关闭全部</Button></div>
          <PageState v-if="connections.length === 0" :model="{ type: 'empty', title: '没有活动连接' }" />
          <VirtualListViewport v-else :items="connections" :get-item-key="(_index, item) => item.id" :estimate-size="62"><template #default="{ item }"><div class="connection-row"><div><strong>{{ item.destination }}</strong><small>{{ item.network }} · {{ item.source }}</small></div><IconButton variant="text" aria-label="关闭连接" @click="closeConnection(item.id)"><IconX :size="17" /></IconButton></div></template></VirtualListViewport>
        </section>
        <section v-else-if="tab === 'logs'" class="operation-tab-panel">
          <div class="log-controls"><Segmented :model-value="logChannel" :options="channelOptions" @change="changeLogChannel" /><Segmented :model-value="logView" :options="viewOptions" @change="changeLogView" /></div>
          <div class="section-heading"><h3>{{ logView === 'structured' ? '结构化日志' : '原始日志' }}</h3><div class="log-actions"><IconButton variant="text" aria-label="滚动到顶部" title="滚动到顶部" @click="logViewport?.scrollToStart()"><IconArrowUp :size="17" /></IconButton><IconButton variant="text" aria-label="滚动到底部" title="滚动到底部" @click="logViewport?.scrollToEnd()"><IconArrowDown :size="17" /></IconButton><IconButton variant="text" aria-label="清除全部日志" title="清除全部日志" @click="confirmAction = 'logs'"><IconTrash :size="17" /></IconButton></div></div>
          <PageState v-if="logs.length === 0" :model="{ type: 'empty', title: '当前频道没有日志' }" />
          <VirtualListViewport v-else ref="logViewport" :items="logs" :get-item-key="(_index, item) => item.id" :estimate-size="logView === 'raw' ? 96 : 68"><template #default="{ item }"><pre v-if="logView === 'raw'" class="raw-log-row">{{ item.raw }}</pre><div v-else class="log-row"><strong>{{ item.kind }}</strong><span>{{ item.message }}</span><small>{{ item.time }}</small></div></template></VirtualListViewport>
        </section>
        <section v-else-if="tab === 'system'" class="operation-tab-panel">
          <div class="operation-grid"><div v-for="item in systemRows" :key="item.label"><span>{{ item.label }}</span><strong>{{ item.value }}</strong></div></div>
          <div class="command-band"><Button variant="outline" @click="updateRuleset"><IconBolt :size="17" />更新规则集</Button><Button variant="outline" @click="createDiagnostic"><IconBug :size="17" />生成诊断包</Button></div>
          <div class="topology-summary"><h3>网络拓扑</h3><span>接管模式：{{ topology?.capture_mode ?? '--' }}</span><span>IPv4：{{ topology?.ipv4 ?? '--' }}</span><span>IPv6：{{ topology?.ipv6 ?? '--' }}</span></div>
        </section>
        <section v-else class="operation-tab-panel"><div class="command-band"><Button variant="outline" @click="exportBackup"><IconArchive :size="17" />导出备份</Button></div><Textarea v-model="restoreText" variant="outline" placeholder="粘贴 NetHop 配置备份" :maxlength="1048576" :min-rows="8" :max-rows="20" /><Button variant="primary" :disabled="!restoreText" @click="restoreBackup">验证并恢复</Button></section>
    </template>
    <Dialog :model-value="Boolean(confirmAction)" :aria-label="confirmAction === 'logs' ? '清除全部日志' : '关闭全部连接'" @update:model-value="(value) => { if (!value) confirmAction = undefined; }">
      <template #title>{{ confirmAction === 'logs' ? '清除全部日志' : '关闭全部连接' }}</template>
      <p>{{ confirmAction === 'logs' ? '服务、订阅和内核日志都会清除，且无法恢复。' : '所有当前代理连接都将中断，是否继续？' }}</p>
      <template #actions>
        <Button variant="outline" @click="confirmAction = undefined">取消</Button>
        <Button variant="danger" @click="runConfirmed">确认</Button>
      </template>
    </Dialog>
  </section>
</template>
