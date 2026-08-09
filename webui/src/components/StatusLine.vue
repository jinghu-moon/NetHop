<script setup lang="ts">
import { computed } from "vue";
import { IconCircleCheck, IconAlertTriangle, IconCircleX, IconLoader2 } from "@tabler/icons-vue";
const props = withDefaults(defineProps<{ status: "running" | "stopped" | "degraded" | "stale" | "error" | "loading"; label: string }>(), { status: "stopped" });
const icon = computed(() => ({ running: IconCircleCheck, stopped: IconCircleX, degraded: IconAlertTriangle, stale: IconAlertTriangle, error: IconCircleX, loading: IconLoader2 }[props.status]));
</script>
<template><div class="status-line" :data-status="status"><component :is="icon" :size="17" /><span>{{ label }}</span></div></template>
