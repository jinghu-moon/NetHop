<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { HostAdapter, PackageInfo } from "@/bridge/host";
import { originalPackageIconSource } from "@/bridge/package-icon";

const props = defineProps<{ host: HostAdapter; app: PackageInfo }>();
const failed = ref(false);
const fallback = computed(() => (props.app.appLabel || props.app.packageName).slice(0, 1).toUpperCase());
const original = computed(() => originalPackageIconSource(props.host.capability.kind, props.app.packageName, props.app.lastUpdateTimeMs));
watch(() => [props.app.packageName, props.app.lastUpdateTimeMs] as const, () => { failed.value = false; }, { immediate: true });
const source = computed(() => original.value);
</script>
<template><div class="application-icon" aria-hidden="true"><span v-if="!source || failed">{{ fallback }}</span><img v-else :src="source" alt="" @error="failed = true" /></div></template>
<style scoped>
.application-icon { width: 42px; height: 42px; min-width: 42px; border-radius: 8px; display: grid; place-items: center; overflow: hidden; background: var(--surface-muted, #eef1f5); color: var(--text-secondary, #667085); font-weight: 700; }
.application-icon img { width: 100%; height: 100%; object-fit: contain; display: block; }
</style>
