<script setup lang="ts">
import { IconAlertTriangle } from "@tabler/icons-vue";
defineProps<{ content: string }>();
</script>
<template><aside class="nh-notice-bar" role="status"><IconAlertTriangle :size="18" aria-hidden="true" /><span>{{ content }}</span><div v-if="$slots.action" class="nh-notice-bar__action"><slot name="action" /></div></aside></template>
<style scoped>
.nh-notice-bar { display: flex; min-height: 44px; align-items: center; padding: max(8px, env(safe-area-inset-top)) 12px 8px; border-bottom: 1px solid color-mix(in srgb, var(--warning) 35%, var(--border-default)); color: var(--warning-on-container); background: var(--warning-container); gap: 8px; font-size: 12px; }
.nh-notice-bar > svg { flex: 0 0 auto; }
.nh-notice-bar > span { min-width: 0; flex: 1; line-height: 1.4; }
.nh-notice-bar__action { flex: 0 0 auto; }
</style>
