<script setup lang="ts">
import { computed } from "vue";
import { IconCircleCheck, IconAlertTriangle, IconCircleX, IconLoader2 } from "@tabler/icons-vue";
const props = withDefaults(defineProps<{ status: "running" | "stopped" | "degraded" | "stale" | "error" | "loading"; label: string }>(), { status: "stopped" });
const icon = computed(() => ({ running: IconCircleCheck, stopped: IconCircleX, degraded: IconAlertTriangle, stale: IconAlertTriangle, error: IconCircleX, loading: IconLoader2 }[props.status]));
</script>
<template><div class="status-line" :data-status="status"><component :is="icon" :size="17" /><span>{{ label }}</span></div></template>
<style scoped>
.status-line { display: inline-flex; align-items: center; flex: 0 0 auto; gap: 5px; color: var(--nh-muted); font-size: 11px; line-height: 1; }
.status-line svg { display: block; flex: 0 0 auto; }
.status-line[data-status="running"] { color: var(--nh-success); }
.status-line[data-status="degraded"], .status-line[data-status="stale"] { color: var(--nh-warning); }
.status-line[data-status="error"] { color: var(--nh-danger); }
</style>
