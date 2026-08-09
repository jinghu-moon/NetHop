<script setup lang="ts">
import { IconChartLine } from "@tabler/icons-vue";
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { TrafficPoint } from "@/runtime/traffic-ring";
const props = withDefaults(defineProps<{ points: readonly TrafficPoint[]; compact?: boolean }>(), { compact: false });
const canvas = ref<HTMLCanvasElement>();
let observer: ResizeObserver | undefined;
const hasTraffic = computed(() => props.points.some((point) => point.down > 0 || point.up > 0));
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
  const max = Math.max(1, ...props.points.flatMap((point) => [point.down, point.up]));
  ctx.strokeStyle = border; ctx.lineWidth = ratio;
  for (const fraction of [0.33, 0.66]) { const y = Math.round(element.height * fraction); ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(element.width, y); ctx.stroke(); }
  const drawSeries = (values: readonly number[], color: string): void => {
    ctx.beginPath();
    values.forEach((value, index) => { const x = (index / (values.length - 1)) * element.width; const y = element.height - (value / max) * (element.height - 8 * ratio) - 4 * ratio; if (index === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); });
    ctx.strokeStyle = color; ctx.lineWidth = 2 * ratio; ctx.lineJoin = "round"; ctx.lineCap = "round"; ctx.stroke();
  };
  drawSeries(props.points.map((point) => point.down), accent);
  drawSeries(props.points.map((point) => point.up), success);
}
watch(() => props.points, draw, { deep: false });
onMounted(() => { draw(); observer = new ResizeObserver(draw); if (canvas.value) observer.observe(canvas.value); });
onBeforeUnmount(() => observer?.disconnect());
</script>
<template><div class="sparkline-wrap" :data-empty="!hasTraffic" :data-compact="compact"><canvas v-show="hasTraffic" ref="canvas" /><div v-if="!hasTraffic" class="sparkline-empty"><IconChartLine :size="22" /><span>暂无流量样本</span></div></div></template>
