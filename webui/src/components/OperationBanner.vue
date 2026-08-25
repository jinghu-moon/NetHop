<script setup lang="ts">
import { IconAlertTriangle, IconCircleCheck, IconLoader2 } from "@tabler/icons-vue";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { OperationPhase } from "@/runtime/operation";
const props = defineProps<{ phase: OperationPhase; message?: string }>();
const emit = defineEmits<{ dismiss: [] }>();
const theme = computed<"info" | "success" | "warning" | "error">(() => {
  if (props.phase === "success") return "success";
  if (props.phase === "failure" || props.phase === "timeout" || props.phase === "conflict") return "error";
  if (props.phase === "running" || props.phase === "accepted") return "info";
  return "warning";
});
const duration = computed(() => {
  if (props.phase === "running" || props.phase === "accepted") return 0;
  if (props.phase === "success") return 2_200;
  if (props.phase === "failure" || props.phase === "timeout" || props.phase === "conflict") return 3_500;
  return 2_500;
});
const displayMessage = computed(() => {
  const message = props.message?.trim();
  if (message) return message;
  return ({
    accepted: "正在提交，请稍候",
    running: "正在处理中，请稍候",
    success: "操作已完成",
    failure: "操作失败",
    conflict: "配置发生冲突",
    timeout: "操作超时",
    idle: "",
  } satisfies Record<OperationPhase, string>)[props.phase];
});
const visible = ref(false);
let revision = 0;
let timer: number | undefined;
const show = async (): Promise<void> => {
  if (timer !== undefined) window.clearTimeout(timer);
  const current = ++revision;
  visible.value = false;
  await nextTick();
  if (current !== revision) return;
  visible.value = props.phase !== "idle";
  if (visible.value && duration.value > 0) timer = window.setTimeout(dismiss, duration.value);
};
watch(() => [props.phase, displayMessage.value], show);
onMounted(show);
onBeforeUnmount(() => { if (timer !== undefined) window.clearTimeout(timer); });
function dismiss(): void { visible.value = false; emit("dismiss"); }
</script>
<template>
  <Transition name="operation-message"><div v-if="visible && phase !== 'idle'" class="operation-message" :data-theme="theme" :data-phase="phase" :role="theme === 'error' ? 'alert' : 'status'" aria-live="polite"><IconLoader2 v-if="phase === 'running' || phase === 'accepted'" class="operation-message__spinner" :size="18" aria-hidden="true" /><IconCircleCheck v-else-if="phase === 'success'" :size="18" aria-hidden="true" /><IconAlertTriangle v-else :size="18" aria-hidden="true" /><span>{{ displayMessage }}</span></div></Transition>
</template>

<style scoped>
@keyframes operation-message-spin {
  to { transform: rotate(360deg); }
}

.operation-message__spinner {
  animation: operation-message-spin 800ms linear infinite;
  transform-origin: center;
}
.operation-message { position: fixed; z-index: 20000; top: max(12px, calc(env(safe-area-inset-top) + 8px)); left: 50%; display: flex; max-width: calc(100vw - 32px); min-height: 40px; align-items: center; padding: 9px 12px; border: 1px solid var(--border-default); border-radius: 7px; color: var(--text-primary); background: var(--surface); box-shadow: var(--shadow-2); gap: 8px; font-size: 12px; transform: translateX(-50%); }
.operation-message[data-theme="success"] { border-color: color-mix(in srgb, var(--success) 45%, var(--border-default)); }
.operation-message[data-theme="warning"] { border-color: color-mix(in srgb, var(--warning) 45%, var(--border-default)); }
.operation-message[data-theme="error"] { border-color: color-mix(in srgb, var(--error) 45%, var(--border-default)); }
.operation-message-enter-active, .operation-message-leave-active { transition: opacity .14s ease, transform .14s ease; }
.operation-message-enter-from, .operation-message-leave-to { opacity: 0; transform: translate(-50%, -6px); }

@media (prefers-reduced-motion: reduce) {
  .operation-message__spinner { animation: none; }
}
</style>
