<script setup lang="ts">
import { computed } from "vue";
const props = defineProps<{ label: string; value: string | number | undefined; unit?: string }>();
const display = computed(() => props.value === undefined || props.value === "" ? "--" : String(props.value));
</script>
<template><div class="metric-value"><span>{{ label }}</span><strong><b>{{ display }}</b><small v-if="unit">{{ unit }}</small></strong></div></template>
