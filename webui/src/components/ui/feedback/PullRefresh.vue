<script setup lang="ts">
import { computed, ref } from "vue";
import { IconRefresh } from "@tabler/icons-vue";

const props = withDefaults(defineProps<{ modelValue: boolean; disabled?: boolean; threshold?: number }>(), { disabled: false, threshold: 64 });
const emit = defineEmits<{ "update:modelValue": [value: boolean]; refresh: [] }>();
const root = ref<HTMLElement>();
const startY = ref<number>();
const pull = ref(0);
const indicatorStyle = computed(() => ({ transform: `translate(-50%, ${Math.min(pull.value, 76) - 32}px)`, opacity: String(Math.min(1, pull.value / 28)) }));

function touchStart(event: TouchEvent): void {
  if (props.disabled || props.modelValue || (root.value?.scrollTop ?? 0) > 0) return;
  startY.value = event.touches[0]?.clientY;
}
function touchMove(event: TouchEvent): void {
  if (startY.value === undefined) return;
  const y = event.touches[0]?.clientY;
  if (y === undefined) return;
  const distance = y - startY.value;
  if (distance <= 0) { pull.value = 0; return; }
  pull.value = Math.min(96, distance * .55);
  if (pull.value > 4) event.preventDefault();
}
function touchEnd(): void {
  const shouldRefresh = pull.value >= props.threshold && !props.disabled && !props.modelValue;
  startY.value = undefined;
  pull.value = 0;
  if (!shouldRefresh) return;
  emit("update:modelValue", true);
  emit("refresh");
}
</script>
<template><div ref="root" class="nh-pull-refresh" :data-refreshing="modelValue" @touchstart="touchStart" @touchmove="touchMove" @touchend="touchEnd" @touchcancel="touchEnd"><div class="nh-pull-refresh__indicator" :style="indicatorStyle" role="status" :aria-label="modelValue ? '正在刷新' : '下拉刷新'"><IconRefresh :class="{ 'nh-pull-refresh__spinner': modelValue }" :size="18" aria-hidden="true" /></div><slot /></div></template>
<style scoped>
.nh-pull-refresh { position: relative; min-height: 0; touch-action: pan-y; }
.nh-pull-refresh__indicator { position: absolute; z-index: 2; top: 0; left: 50%; display: grid; width: 30px; height: 30px; place-items: center; border: 1px solid var(--border-default); border-radius: 50%; color: var(--text-secondary); background: var(--surface); box-shadow: var(--shadow-1); pointer-events: none; transform: translate(-50%, -32px); }
.nh-pull-refresh[data-refreshing="true"] .nh-pull-refresh__indicator { opacity: 1 !important; transform: translate(-50%, 8px) !important; }
.nh-pull-refresh__spinner { animation: nh-pull-refresh-spin .75s linear infinite; }
@keyframes nh-pull-refresh-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .nh-pull-refresh__spinner { animation: none; } }
</style>
