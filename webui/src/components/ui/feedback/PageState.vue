<script setup lang="ts">
import { IconAlertTriangle, IconCircleCheck, IconInbox, IconLoader2 } from "@tabler/icons-vue";
import Button from "../primitives/Button.vue";

export type PageStateModel =
  | { readonly type: "loading"; readonly title?: string; readonly detail?: string }
  | { readonly type: "empty"; readonly title: string; readonly detail?: string }
  | { readonly type: "error"; readonly title: string; readonly detail?: string }
  | { readonly type: "warning"; readonly title: string; readonly detail?: string }
  | { readonly type: "ready" };

const props = defineProps<{
  model: PageStateModel;
  actionLabel?: string;
}>();

const emit = defineEmits<{ action: [] }>();
</script>

<template>
  <div
    v-if="model.type !== 'ready'"
    class="nh-page-state"
    :data-state="model.type"
    :role="model.type === 'error' ? 'alert' : 'status'"
    :aria-live="model.type === 'error' ? 'assertive' : 'polite'"
    :aria-busy="model.type === 'loading' ? 'true' : undefined"
  >
    <IconLoader2 v-if="model.type === 'loading'" class="nh-page-state__icon nh-page-state__spinner" :size="28" aria-hidden="true" />
    <IconInbox v-else-if="model.type === 'empty'" class="nh-page-state__icon" :size="32" aria-hidden="true" />
    <IconAlertTriangle v-else-if="model.type === 'error' || model.type === 'warning'" class="nh-page-state__icon" :size="32" aria-hidden="true" />
    <IconCircleCheck v-else class="nh-page-state__icon" :size="32" aria-hidden="true" />
    <strong v-if="model.title">{{ model.title }}</strong>
    <span v-if="model.detail">{{ model.detail }}</span>
    <div v-if="$slots.action || actionLabel" class="nh-page-state__action">
      <slot name="action">
        <Button variant="primary" @click="emit('action')">{{ actionLabel }}</Button>
      </slot>
    </div>
  </div>
</template>

<style scoped>
.nh-page-state { display: flex; min-height: 220px; align-items: center; justify-content: center; padding: 32px 12px; flex-direction: column; gap: 8px; color: var(--text-secondary); text-align: center; }
.nh-page-state__icon { color: var(--text-placeholder); }
.nh-page-state[data-state="error"] .nh-page-state__icon { color: var(--error); }
.nh-page-state[data-state="warning"] .nh-page-state__icon { color: var(--warning); }
.nh-page-state strong { color: var(--text-primary); font-size: 14px; line-height: 1.35; }
.nh-page-state span { max-width: 32em; font-size: 12px; line-height: 1.45; }
.nh-page-state__action { display: flex; margin-top: 4px; justify-content: center; }
.nh-page-state__spinner { animation: nh-page-state-spin .8s linear infinite; }
@keyframes nh-page-state-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .nh-page-state__spinner { animation: none; } }
</style>
