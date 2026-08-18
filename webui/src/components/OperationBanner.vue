<script setup lang="ts">
import { IconAlertTriangle, IconCircleCheck, IconLoader2 } from "@tabler/icons-vue";
import { MessagePlugin as TMessage } from "tdesign-mobile-vue";
import { computed, nextTick, onMounted, ref, watch } from "vue";
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
const show = async (): Promise<void> => {
  const current = ++revision;
  visible.value = false;
  await nextTick();
  if (current === revision) visible.value = props.phase !== "idle";
};
watch(() => [props.phase, displayMessage.value], show);
onMounted(show);
const dismiss = (): void => { visible.value = false; emit("dismiss"); };
</script>
<template>
  <TMessage v-if="phase !== 'idle'" :key="`${phase}:${displayMessage}`" v-model:visible="visible" class="operation-message" :theme="theme" :content="displayMessage" :duration="duration" :offset="[0, 16]" :close-btn="false" :marquee="false" single :z-index="20000" :data-phase="phase" @duration-end="dismiss">
    <template #icon><IconLoader2 v-if="phase === 'running' || phase === 'accepted'" class="operation-message__spinner" :size="18" aria-hidden="true" /><IconCircleCheck v-else-if="phase === 'success'" :size="18" aria-hidden="true" /><IconAlertTriangle v-else :size="18" aria-hidden="true" /></template>
  </TMessage>
</template>

<style scoped>
@keyframes operation-message-spin {
  to { transform: rotate(360deg); }
}

.operation-message__spinner {
  animation: operation-message-spin 800ms linear infinite;
  transform-origin: center;
}

@media (prefers-reduced-motion: reduce) {
  .operation-message__spinner { animation: none; }
}
</style>
