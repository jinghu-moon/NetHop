<script setup lang="ts">
import { IconActivityHeartbeat, IconArrowDown, IconArrowUp, IconBolt, IconChevronRight, IconClock, IconCpu, IconPower } from "@tabler/icons-vue";
import { computed, onActivated, onDeactivated, ref } from "vue";
import { RouterLink } from "vue-router";
import OperationBanner from "@/components/OperationBanner.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";
import TrafficSparkline from "@/components/TrafficSparkline.vue";
import { runJson } from "@/bridge/command";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import Switch from "@/components/ui/primitives/Switch.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import { validatedQuery } from "@/model/client";
import { parseConfig, parseNodeList, parseRuntimeMetrics, parseStatus, parseTraffic, type RuntimeMetricsDto } from "@/model/dto";
import { activeNodeView } from "@/model/node-view";
import { presentServiceStatus } from "@/model/service-presentation";
import { liveTrafficPoints } from "@/runtime/live-store";
import { createActionLock, createOperationStore } from "@/runtime/operation";
import { uiStores } from "@/runtime/store";
import { TrafficRing } from "@/runtime/traffic-ring";

type OutboundMode = "rule" | "global" | "direct";
type CaptureMode = "auto" | "tproxy" | "tun";

const host = useHost();
const status = computed(() => uiStores.session.status.value);
const statusLoadFailed = ref(false);
const service = computed(() => presentServiceStatus(status.value, statusLoadFailed.value));
const metrics = ref<RuntimeMetricsDto>();
const running = computed(() => service.value.phase === "running");
const pending = ref(false);
const modePending = ref(false);
const capturePending = ref(false);
const outboundMode = ref<OutboundMode>("rule");
const committedMode = ref<OutboundMode>("rule");
const captureMode = ref<CaptureMode>("auto");
const committedCaptureMode = ref<CaptureMode>("auto");
const modeReady = ref(false);
const operations = createOperationStore();
const lock = createActionLock();
const ring = new TrafficRing(60);
const fallbackPoints = ref(ring.snapshot());
const points = computed(() => liveTrafficPoints.value.length > 0 ? liveTrafficPoints.value : fallbackPoints.value);
const currentTraffic = computed(() => points.value.at(-1) ?? { up: 0, down: 0, state: "gap" as const });
const selection = computed(() => uiStores.runtime.selection.value);
const sourceNames = computed(() => Object.fromEntries(uiStores.runtime.subscriptionOrder.value.map((id) => [id, uiStores.runtime.subscriptionsById.value[id]?.name ?? id])));
const nodeSummary = computed(() => activeNodeView(selection.value, uiStores.runtime.nodesById.value, sourceNames.value, running.value));
const currentNode = computed(() => nodeSummary.value.kind === "node" ? nodeSummary.value.node : undefined);
const nodeLatency = computed(() => nodeSummary.value.kind === "node" ? nodeSummary.value.latency.text : "--");
const nodeStateLabel = computed(() => nodeSummary.value.kind === "node" ? nodeSummary.value.node.name : nodeSummary.value.title);
const qualityTesting = ref(false);
let metricsTimer: number | undefined;
let metricsPollingActive = false;
let metricsRefreshing = false;
const modeDescription = computed(() => ({ rule: "按路由规则分流", global: "全部流量代理", direct: "全部流量直连" })[outboundMode.value]);
const captureDescription = computed(() => ({ auto: "使用 daemon 默认接管策略", tproxy: "通过内核透明代理接管", tun: "通过虚拟网络接口接管" })[captureMode.value]);
const modeOptions: Array<{ value: OutboundMode; label: string }> = [
  { value: "rule", label: "规则" },
  { value: "global", label: "全局" },
  { value: "direct", label: "直连" },
];
const captureOptions: Array<{ value: CaptureMode; label: string }> = [
  { value: "auto", label: "自动" },
  { value: "tproxy", label: "TPROXY" },
  { value: "tun", label: "TUN" },
];

function formatRate(bytes: number, available = true): { value: string; unit: string } {
  if (!available) return { value: "--", unit: "" };
  if (bytes >= 1024 ** 3) return { value: (bytes / 1024 ** 3).toFixed(bytes >= 10 * 1024 ** 3 ? 0 : 1), unit: "GB/s" };
  if (bytes >= 1024 * 1024) return { value: (bytes / 1024 / 1024).toFixed(bytes >= 10 * 1024 * 1024 ? 0 : 1), unit: "MB/s" };
  if (bytes >= 1024) return { value: (bytes / 1024).toFixed(bytes >= 10 * 1024 ? 0 : 1), unit: "KB/s" };
  return { value: String(Math.max(0, Math.round(bytes))), unit: "B/s" };
}

function formatBytes(bytes: number | undefined): string {
  if (bytes === undefined) return "--";
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatDuration(seconds: number | undefined): string {
  if (seconds === undefined) return "--";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours} 小时 ${minutes} 分`;
  return `${minutes} 分钟`;
}

function formatCpu(percent: number | undefined): string {
  if (percent === undefined) return "--";
  return `${percent.toFixed(percent >= 10 ? 0 : 1)}%`;
}

function modeFrom(document: Readonly<Record<string, unknown>>): OutboundMode | undefined {
  const proxy = document.proxy;
  if (!proxy || typeof proxy !== "object" || Array.isArray(proxy)) return undefined;
  const value = (proxy as Readonly<Record<string, unknown>>).outbound_mode;
  return value === "rule" || value === "global" || value === "direct" ? value : undefined;
}

function captureModeFrom(document: Readonly<Record<string, unknown>>): CaptureMode | undefined {
  const network = document.network;
  if (!network || typeof network !== "object" || Array.isArray(network)) return undefined;
  const value = (network as Readonly<Record<string, unknown>>).capture_mode;
  return value === "auto" || value === "tproxy" || value === "tun" ? value : undefined;
}

const downloadRate = computed(() => formatRate(currentTraffic.value.down, currentTraffic.value.state === "ok"));
const uploadRate = computed(() => formatRate(currentTraffic.value.up, currentTraffic.value.state === "ok"));

async function refresh(): Promise<void> {
  const [statusResult] = await Promise.allSettled([
    validatedQuery(host, { id: "status.get" }, parseStatus),
  ]);
  if (statusResult.status === "fulfilled") {
    uiStores.session.setStatus(statusResult.value);
    statusLoadFailed.value = false;
  } else if (status.value === undefined) {
    statusLoadFailed.value = true;
  }

  const [trafficResult, configResult, nodesResult, metricsResult] = await Promise.allSettled([
    validatedQuery(host, { id: "traffic.get" }, parseTraffic),
    validatedQuery(host, { id: "config.get" }, parseConfig),
    validatedQuery(host, { id: "node.list", limit: 128 }, parseNodeList),
    validatedQuery(host, { id: "metrics.get" }, parseRuntimeMetrics),
  ]);
  if (trafficResult.status === "fulfilled") {
    ring.push(trafficResult.value);
    fallbackPoints.value = ring.snapshot();
  }
  if (nodesResult.status === "fulfilled") uiStores.runtime.loadNodeSnapshot(nodesResult.value);
  if (metricsResult.status === "fulfilled") metrics.value = metricsResult.value;
  if (configResult.status === "fulfilled") {
    uiStores.config.load(configResult.value);
    const configuredMode = modeFrom(configResult.value.document);
    if (configuredMode) {
      outboundMode.value = configuredMode;
      committedMode.value = configuredMode;
      modeReady.value = true;
    } else {
      modeReady.value = false;
    }
    const configuredCaptureMode = captureModeFrom(configResult.value.document);
    if (configuredCaptureMode) {
      captureMode.value = configuredCaptureMode;
      committedCaptureMode.value = configuredCaptureMode;
    }
  } else {
    modeReady.value = false;
  }
  if ([statusResult, trafficResult, configResult, nodesResult, metricsResult].every((result) => result.status === "rejected")) throw new Error("overview unavailable");
}

async function refreshStatusOnly(): Promise<void> {
  const next = await validatedQuery(host, { id: "status.get" }, parseStatus);
  uiStores.session.setStatus(next);
  statusLoadFailed.value = false;
}

async function refreshMetrics(): Promise<void> {
  if (metricsRefreshing) return;
  metricsRefreshing = true;
  try {
    metrics.value = await validatedQuery(host, { id: "metrics.get" }, parseRuntimeMetrics);
  } finally {
    metricsRefreshing = false;
  }
}

function scheduleMetricsRefresh(): void {
  if (!metricsPollingActive || metricsTimer !== undefined) return;
  metricsTimer = window.setTimeout(() => {
    metricsTimer = undefined;
    if (!metricsPollingActive) return;
    void refreshMetrics()
      .catch(() => undefined)
      .finally(scheduleMetricsRefresh);
  }, 3_000);
}

async function testProxyQuality(): Promise<void> {
  if (qualityTesting.value) return;
  qualityTesting.value = true;
  try {
    await runJson(host, { id: "node.test-all" });
    await refresh();
  } finally { qualityTesting.value = false; }
}

async function changeCaptureMode(context: { value: string | number }): Promise<void> {
  const next = context.value;
  if ((next !== "auto" && next !== "tproxy" && next !== "tun") || next === committedCaptureMode.value || capturePending.value) return;
  const previous = committedCaptureMode.value;
  captureMode.value = next;
  capturePending.value = true;
  operations.begin("capture-mode", "network");
  operations.update("capture-mode", "running");
  try {
    await uploadPrivatePayload(host, "config", "config-mutate", JSON.stringify({
      expected_config_digest: uiStores.config.baseDigest.value,
      mutation: { type: "set_scalar_field", field_id: "network.capture_mode", value: next },
    }));
    committedCaptureMode.value = next;
    await refresh();
    operations.update("capture-mode", "success", { message: `接管方式已切换为${captureOptions.find((item) => item.value === next)?.label ?? next}` });
  } catch {
    captureMode.value = previous;
    operations.update("capture-mode", "failure", { message: "接管方式切换失败，已恢复原设置" });
  } finally {
    capturePending.value = false;
  }
}

const toggle = async (value: unknown): Promise<void> => {
  const desired = value === true || value === "true";
  await lock.run("service", async () => {
    const uiStartedAt = performance.now();
    pending.value = true;
    operations.begin("service", "service");
    operations.update("service", "running");
    try {
      const result = await runJson(host, { id: desired ? "capture.enable" : "capture.disable", wait: true });
      const uiDurationMs = Math.max(0, Math.round(performance.now() - uiStartedAt));
      operations.update("service", "success", { message: `${desired ? "代理已启动" : "代理已关闭"}（bridge ${result.durationMs} ms · UI ${uiDurationMs} ms）` });
      void refreshStatusOnly().catch(() => undefined);
    } catch {
      operations.update("service", "failure", { code: "NH-UI-OPERATION", message: "代理状态切换失败" });
    } finally {
      pending.value = false;
    }
  });
};

async function changeMode(context: { value: string | number }): Promise<void> {
  const next = context.value;
  if ((next !== "rule" && next !== "global" && next !== "direct") || next === committedMode.value || modePending.value) return;
  const previous = committedMode.value;
  outboundMode.value = next;
  modePending.value = true;
  operations.begin("proxy-mode", "proxy");
  operations.update("proxy-mode", "running");
  try {
    await uploadPrivatePayload(host, "config", "config-mutate", JSON.stringify({
      expected_config_digest: uiStores.config.baseDigest.value,
      mutation: { type: "set_scalar_field", field_id: "proxy.outbound_mode", value: next },
    }));
    await refresh();
    operations.update("proxy-mode", "success", { message: `代理模式已切换为${modeOptions.find((item) => item.value === next)?.label ?? next}` });
  } catch {
    outboundMode.value = previous;
    operations.update("proxy-mode", "failure", { message: "代理模式切换失败，已恢复原设置" });
  } finally {
    modePending.value = false;
  }
}

onActivated(() => {
  void refresh().catch(() => { modeReady.value = false; });
  metricsPollingActive = true;
  scheduleMetricsRefresh();
});
onDeactivated(() => {
  metricsPollingActive = false;
  if (metricsTimer !== undefined) window.clearTimeout(metricsTimer);
  metricsTimer = undefined;
});
</script>

<template>
  <section class="page overview-page">
    <div class="page-heading overview-heading"><div><h2>概览</h2><p>代理状态与实时流量</p></div></div>

    <OperationBanner v-if="operations.byId['service']" :phase="operations.byId['service']!.phase" :message="operations.byId['service']!.message ?? ''" @dismiss="operations.clear('service')" />
    <OperationBanner v-if="operations.byId['proxy-mode']" :phase="operations.byId['proxy-mode']!.phase" :message="operations.byId['proxy-mode']!.message ?? ''" @dismiss="operations.clear('proxy-mode')" />
    <OperationBanner v-if="operations.byId['capture-mode']" :phase="operations.byId['capture-mode']!.phase" :message="operations.byId['capture-mode']!.message ?? ''" @dismiss="operations.clear('capture-mode')" />

    <section class="service-panel">
      <div class="service-control">
        <div class="service-summary">
          <span class="service-symbol" :data-running="running" :data-state="service.phase"><IconPower :size="18" /></span>
          <div><strong>{{ service.title }}</strong><span>{{ service.description }}</span></div>
        </div>
        <Switch :model-value="service.switchValue" :loading="pending || service.switchLoading" :disabled="pending || service.switchDisabled" :aria-label="service.title" @change="toggle" />
      </div>
    </section>

    <section class="overview-mode">
      <div class="overview-section-heading"><strong>代理模式</strong><span>{{ modeDescription }}</span></div>
      <Segmented :model-value="outboundMode" :options="modeOptions" :disabled="!modeReady || modePending" @change="changeMode" />
    </section>

    <section class="overview-mode capture-mode">
      <div class="overview-section-heading"><strong>接管方式</strong><span>{{ captureDescription }}</span></div>
      <Segmented :model-value="captureMode" :options="captureOptions" :disabled="!modeReady || capturePending" @change="changeCaptureMode" />
    </section>

    <section class="traffic-section">
      <div class="traffic-heading">
        <div><h3>实时流量</h3><span>最近 60 秒</span></div>
        <div class="traffic-rates">
          <div class="traffic-rate" data-direction="download">
            <IconArrowDown class="traffic-rate__icon" :size="13" aria-hidden="true" />
            <strong>{{ downloadRate.value }}</strong>
            <small>{{ downloadRate.unit }}</small>
          </div>
          <div class="traffic-rate" data-direction="upload">
            <IconArrowUp class="traffic-rate__icon" :size="13" aria-hidden="true" />
            <strong>{{ uploadRate.value }}</strong>
            <small>{{ uploadRate.unit }}</small>
          </div>
        </div>
      </div>
      <TrafficSparkline :points="points" compact />
    </section>

    <div class="overview-insight-grid">
      <section class="overview-insight-card proxy-quality-card node-summary">
        <RouterLink to="/nodes" class="proxy-quality-link">
          <div class="insight-heading">
            <span class="insight-icon"><IconActivityHeartbeat :size="17" /></span>
            <strong>代理质量</strong>
            <IconChevronRight :size="17" />
          </div>
          <div class="insight-primary">
            <strong>{{ nodeStateLabel }}</strong>
            <span class="node-latency" :data-ready="currentNode?.latencyMs !== undefined">{{ nodeLatency }}</span>
          </div>
          <div class="insight-detail">
            <span>{{ selection?.intent.mode === "auto" ? "自动优选" : "手动选择" }} · {{ currentNode?.protocol?.toUpperCase() ?? "--" }}</span>
            <span>{{ metrics?.publicIp ?? "未探测出口" }}</span>
          </div>
        </RouterLink>
        <IconButton class="quality-test-button" size="s" variant="text" aria-label="测试全部节点" :loading="qualityTesting" title="测试全部节点" @click="testProxyQuality"><IconBolt :size="17" /></IconButton>
      </section>

      <section class="overview-insight-card runtime-card">
        <div class="insight-heading">
          <span class="insight-icon runtime-icon"><IconClock :size="17" /></span>
          <strong>本次运行</strong>
        </div>
        <strong class="runtime-duration">{{ formatDuration(metrics?.uptimeSeconds) }}</strong>
        <div class="runtime-traffic">
          <span><IconArrowDown :size="12" />{{ formatBytes(metrics?.downloadBytes) }}</span>
          <span><IconArrowUp :size="12" />{{ formatBytes(metrics?.uploadBytes) }}</span>
        </div>
      </section>

      <section class="overview-insight-card resource-card">
        <div class="insight-heading">
          <span class="insight-icon resource-icon"><IconCpu :size="17" /></span>
          <strong>核心资源</strong>
        </div>
        <div class="resource-metrics">
          <div><span>CPU</span><strong>{{ formatCpu(metrics?.core?.cpuPercent) }}</strong></div>
          <div><span>内存</span><strong>{{ formatBytes(metrics?.core?.memoryRssBytes) }}</strong></div>
        </div>
      </section>
    </div>
  </section>
</template>
