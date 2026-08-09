<script setup lang="ts">
import { IconActivityHeartbeat, IconArrowDown, IconArrowUp, IconChevronRight, IconClock, IconPower } from "@tabler/icons-vue";
import { Switch as TSwitch } from "tdesign-mobile-vue";
import { computed, onActivated, ref } from "vue";
import { RouterLink } from "vue-router";
import OperationBanner from "@/components/OperationBanner.vue";
import SegmentedControl from "@/components/SegmentedControl.vue";
import TrafficSparkline from "@/components/TrafficSparkline.vue";
import { runJson } from "@/bridge/command";
import { useHost } from "@/bridge/context";
import { uploadPrivatePayload } from "@/bridge/private-payload";
import { validatedQuery } from "@/model/client";
import { parseConfig, parseNodeList, parseRuntimeMetrics, parseStatus, parseTraffic, type NodeDto, type RuntimeMetricsDto, type StatusDto } from "@/model/dto";
import { liveTrafficPoints } from "@/runtime/live-store";
import { createActionLock, createOperationStore } from "@/runtime/operation";
import { uiStores } from "@/runtime/store";
import { TrafficRing } from "@/runtime/traffic-ring";

type OutboundMode = "rule" | "global" | "direct";
type CaptureMode = "auto" | "tproxy" | "tun";

const host = useHost();
const status = ref<StatusDto>();
const metrics = ref<RuntimeMetricsDto>();
const nodes = ref<readonly NodeDto[]>([]);
const running = computed(() => status.value?.state === "running_tproxy" || status.value?.state === "running_tun");
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
const currentTraffic = computed(() => points.value.at(-1) ?? { up: 0, down: 0 });
const currentNode = computed(() => nodes.value.find((node) => node.selected) ?? nodes.value[0]);
const nodeLatency = computed(() => currentNode.value?.latencyMs === undefined ? "未测速" : `${currentNode.value.latencyMs} ms`);
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

function formatRate(bytes: number): { value: string; unit: string } {
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

const downloadRate = computed(() => formatRate(currentTraffic.value.down));
const uploadRate = computed(() => formatRate(currentTraffic.value.up));

async function refresh(): Promise<void> {
  const [statusResult, trafficResult, configResult, nodesResult, metricsResult] = await Promise.allSettled([
    validatedQuery(host, { id: "status.get" }, parseStatus),
    validatedQuery(host, { id: "traffic.get" }, parseTraffic),
    validatedQuery(host, { id: "config.get" }, parseConfig),
    validatedQuery(host, { id: "node.list", limit: 128 }, parseNodeList),
    validatedQuery(host, { id: "metrics.get" }, parseRuntimeMetrics),
  ]);
  if (statusResult.status === "fulfilled") status.value = statusResult.value;
  if (trafficResult.status === "fulfilled") {
    ring.push(trafficResult.value, Date.now());
    fallbackPoints.value = ring.snapshot();
  }
  if (nodesResult.status === "fulfilled") nodes.value = nodesResult.value;
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
    pending.value = true;
    operations.begin("service", "service");
    operations.update("service", "running");
    try {
      await runJson(host, { id: desired ? "service.start" : "service.stop", wait: true });
      await refresh();
      operations.update("service", "success", { message: desired ? "代理已启动" : "代理已关闭" });
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

onActivated(() => { void refresh().catch(() => { modeReady.value = false; }); });
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
          <span class="service-symbol" :data-running="running"><IconPower :size="18" /></span>
          <div><strong>{{ running ? "代理运行中" : "代理已关闭" }}</strong><span>{{ running ? "流量接管已生效" : "当前网络未经过 NetHop" }}</span></div>
        </div>
        <TSwitch :value="running" :loading="pending" :disabled="pending" @change="toggle" />
      </div>
    </section>

    <section class="overview-mode">
      <div class="overview-section-heading"><strong>代理模式</strong><span>{{ modeDescription }}</span></div>
      <SegmentedControl :model-value="outboundMode" :options="modeOptions" :disabled="!modeReady || modePending" @change="changeMode" />
    </section>

    <section class="overview-mode capture-mode">
      <div class="overview-section-heading"><strong>接管方式</strong><span>{{ captureDescription }}</span></div>
      <SegmentedControl :model-value="captureMode" :options="captureOptions" :disabled="!modeReady || capturePending" @change="changeCaptureMode" />
    </section>

    <section class="traffic-section">
      <div class="traffic-heading">
        <div><h3>实时流量</h3><span>最近 60 秒</span></div>
        <div class="traffic-rates">
          <div class="traffic-rate">
            <IconArrowDown :size="13" />
            <strong>{{ downloadRate.value }} <small>{{ downloadRate.unit }}</small></strong>
          </div>
          <div class="traffic-rate">
            <IconArrowUp :size="13" />
            <strong>{{ uploadRate.value }} <small>{{ uploadRate.unit }}</small></strong>
          </div>
        </div>
      </div>
      <TrafficSparkline :points="points" compact />
    </section>

    <div class="overview-insight-grid">
      <RouterLink to="/nodes" class="overview-insight-card proxy-quality-card node-summary">
        <div class="insight-heading">
          <span class="insight-icon"><IconActivityHeartbeat :size="17" /></span>
          <strong>代理质量</strong>
          <IconChevronRight :size="17" />
        </div>
        <div class="insight-primary">
          <strong>{{ currentNode?.name ?? "暂无可用节点" }}</strong>
          <span class="node-latency" :data-ready="currentNode?.latencyMs !== undefined">{{ nodeLatency }}</span>
        </div>
        <div class="insight-detail">
          <span>{{ currentNode?.protocol?.toUpperCase() ?? "--" }}</span>
          <span>{{ metrics?.publicIp ?? "未探测出口" }}</span>
        </div>
      </RouterLink>

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
    </div>
  </section>
</template>
