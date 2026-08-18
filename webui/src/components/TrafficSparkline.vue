<script setup lang="ts">
import { IconChartLine } from "@tabler/icons-vue";
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { TrafficPoint } from "@/runtime/traffic-ring";
const props = withDefaults(defineProps<{ points: readonly TrafficPoint[]; compact?: boolean }>(), { compact: false });
const canvas = ref<HTMLCanvasElement>();
let observer: ResizeObserver | undefined;
const validPoints = computed(() => props.points.filter((point) => point.state === "ok"));
const hasTraffic = computed(() => validPoints.value.some((point) => point.down > 0 || point.up > 0));
const peakRate = computed(() => Math.max(0, ...validPoints.value.flatMap((point) => [point.down, point.up])));
function niceUpperBound(value: number): number {
  if (value <= 0) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  const normalized = value / magnitude;
  const factor = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return factor * magnitude;
}
const scaleMax = computed(() => niceUpperBound(peakRate.value));
const axis = computed(() => {
  const divisor = scaleMax.value >= 1024 ** 2 ? 1024 ** 2 : scaleMax.value >= 1024 ? 1024 : 1;
  const unit = divisor === 1024 ** 2 ? "MB/s" : divisor === 1024 ? "KB/s" : "B/s";
  const format = (value: number): string => {
    const scaled = value / divisor;
    return Number.isInteger(scaled) ? String(scaled) : scaled.toFixed(1);
  };
  return { top: format(scaleMax.value), middle: format(scaleMax.value / 2), unit };
});
function draw(): void {
  const element = canvas.value; if (!element) return;
  const rect = element.getBoundingClientRect(); const ratio = window.devicePixelRatio || 1;
  element.width = Math.max(1, Math.floor(rect.width * ratio)); element.height = Math.max(1, Math.floor(rect.height * ratio));
  const ctx = element.getContext("2d"); if (!ctx) return; ctx.clearRect(0, 0, element.width, element.height);
  if (props.points.length < 2 || !hasTraffic.value) return;
  const style = getComputedStyle(document.documentElement);
  const accent = style.getPropertyValue("--nh-info").trim() || "#477b94";
  const success = style.getPropertyValue("--nh-success").trim() || "#40805c";
  const border = style.getPropertyValue("--nh-border").trim() || "#e0e0e0";
  const max = scaleMax.value;
  const padding = 4 * ratio;
  const plotHeight = Math.max(1, element.height - 2 * padding);
  ctx.strokeStyle = border; ctx.lineWidth = ratio;
  for (const fraction of [0, 0.5, 1]) { const y = Math.round(padding + plotHeight * fraction); ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(element.width, y); ctx.stroke(); }
  const drawSeries = (value: (point: TrafficPoint) => number, color: string): void => {
    ctx.strokeStyle = color; ctx.lineWidth = 2 * ratio; ctx.lineJoin = "round"; ctx.lineCap = "round";
    let drawing = false;
    props.points.forEach((point, index) => {
      if (point.state !== "ok") { drawing = false; return; }
      const x = (index / (props.points.length - 1)) * element.width;
      const y = padding + (1 - value(point) / max) * plotHeight;
      if (!drawing) { ctx.beginPath(); ctx.moveTo(x, y); drawing = true; } else ctx.lineTo(x, y);
      if (index === props.points.length - 1 || props.points[index + 1]?.state !== "ok") ctx.stroke();
    });
  };
  drawSeries((point) => point.down, accent);
  drawSeries((point) => point.up, success);
}
watch(() => props.points, draw, { deep: false });
onMounted(() => { draw(); observer = new ResizeObserver(draw); if (canvas.value) observer.observe(canvas.value); });
onBeforeUnmount(() => observer?.disconnect());
</script>
<template>
  <div class="sparkline-wrap" :data-empty="!hasTraffic" :data-compact="compact">
    <div v-if="hasTraffic" class="sparkline-axis" aria-hidden="true"><span>{{ axis.top }}</span><span>{{ axis.middle }}</span><span>0</span></div>
    <canvas v-show="hasTraffic" ref="canvas" role="img" :aria-label="`最近流量曲线，纵轴范围 0 至 ${axis.top} ${axis.unit}`" />
    <div v-if="!hasTraffic" class="sparkline-empty"><IconChartLine :size="22" /><span>暂无流量样本</span></div>
  </div>
</template>
