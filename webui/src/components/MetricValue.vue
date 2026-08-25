<script setup lang="ts">
import { computed } from "vue";
const props = defineProps<{ label: string; value: string | number | undefined; unit?: string }>();
const display = computed(() => props.value === undefined || props.value === "" ? "--" : String(props.value));
</script>
<template><div class="metric-value"><span>{{ label }}</span><strong><b>{{ display }}</b><small v-if="unit">{{ unit }}</small></strong></div></template>
<style scoped>
.metric-value { display: flex; min-width: 0; flex-direction: column; gap: 5px; }
.metric-value > span { color: var(--nh-muted); font-size: 11px; }
.metric-value strong { display: flex; min-width: 0; align-items: baseline; gap: 4px; }
.metric-value b { overflow: hidden; color: var(--nh-text); font-family: var(--nh-font-number); font-size: 18px; font-variant-numeric: tabular-nums; letter-spacing: 0; text-overflow: ellipsis; white-space: nowrap; }
.metric-value small { color: var(--nh-muted); font-size: 11px; }
</style>
