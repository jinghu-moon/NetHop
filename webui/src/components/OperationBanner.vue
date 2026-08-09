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
const visible = ref(false);
let revision = 0;
const show = async (): Promise<void> => {
  const current = ++revision;
  visible.value = false;
  await nextTick();
  if (current === revision) visible.value = props.phase !== "idle";
};
watch(() => [props.phase, props.message], show);
onMounted(show);
const dismiss = (): void => { visible.value = false; emit("dismiss"); };
</script>
<template>
  <TMessage v-if="phase !== 'idle'" :key="`${phase}:${message ?? ''}`" v-model:visible="visible" class="operation-message" :theme="theme" :content="message ?? phase" :duration="duration" :offset="[0, 16]" :close-btn="false" :marquee="false" single :z-index="20000" :data-phase="phase" @duration-end="dismiss">
    <template #icon><IconLoader2 v-if="phase === 'running' || phase === 'accepted'" :size="18" /><IconCircleCheck v-else-if="phase === 'success'" :size="18" /><IconAlertTriangle v-else :size="18" /></template>
  </TMessage>
</template>
