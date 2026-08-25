<script setup lang="ts">
import { IconAlertTriangle, IconCircleCheck, IconInfoCircle, IconLoader2, IconX } from "@tabler/icons-vue";
import Button from "../primitives/Button.vue";
import IconButton from "../primitives/IconButton.vue";
import type { ToastAction, ToastItem } from "./toast-types";

defineProps<{ item: ToastItem; progress?: number; entering?: boolean; pulsing?: boolean; shaking?: boolean }>();
const emit = defineEmits<{
  action: [action: ToastAction, toastId: string];
  close: [toastId: string];
  pause: [toastId: string];
  resume: [toastId: string];
}>();
</script>

<template>
  <article
    class="nh-toast"
    :class="[`nh-toast--${item.tone}`, { 'nh-toast--entering': entering, 'nh-toast--pulsing': pulsing, 'nh-toast--shaking': shaking }]"
    :role="item.tone === 'error' ? 'alert' : 'status'"
    :aria-live="item.tone === 'error' ? 'assertive' : 'polite'"
    :data-toast-id="item.id"
    @mouseenter="emit('pause', item.id)"
    @mouseleave="emit('resume', item.id)"
  >
    <IconLoader2 v-if="item.tone === 'loading'" class="nh-toast__icon nh-toast__spinner" :size="18" aria-hidden="true" />
    <IconCircleCheck v-else-if="item.tone === 'success'" class="nh-toast__icon" :size="18" aria-hidden="true" />
    <IconAlertTriangle v-else-if="item.tone === 'warning' || item.tone === 'error'" class="nh-toast__icon" :size="18" aria-hidden="true" />
    <IconInfoCircle v-else class="nh-toast__icon" :size="18" aria-hidden="true" />
    <div class="nh-toast__content">
      <strong>{{ item.message }}</strong>
      <span v-if="item.detail">{{ item.detail }}</span>
    </div>
    <Button v-if="item.action" variant="text" size="s" :disabled="Boolean(item.action.disabled)" @click="emit('action', item.action, item.id)">{{ item.action.label }}</Button>
    <IconButton v-if="item.closable" variant="text" size="s" :aria-label="item.closeLabel || '关闭提示'" @click="emit('close', item.id)"><IconX :size="16" aria-hidden="true" /></IconButton>
    <span v-if="item.showProgress && item.duration && item.duration > 0 && !item.persistent" class="nh-toast__progress" aria-hidden="true">
      <span :style="{ transform: `scaleX(${Math.max(0, Math.min(1, progress ?? 1))})` }" />
    </span>
  </article>
</template>

<style scoped>
.nh-toast { position: relative; display: flex; width: min(100%, 420px); min-height: 44px; align-items: center; padding: 10px 12px; border: 1px solid var(--border-default); border-radius: 7px; color: var(--text-primary); background: var(--surface); box-shadow: var(--shadow-2); gap: 9px; }
.nh-toast--success { border-color: color-mix(in srgb, var(--success) 45%, var(--border-default)); }
.nh-toast--warning { border-color: color-mix(in srgb, var(--warning) 45%, var(--border-default)); }
.nh-toast--error { border-color: color-mix(in srgb, var(--error) 45%, var(--border-default)); }
.nh-toast__icon { flex: 0 0 auto; color: var(--text-secondary); }
.nh-toast--success .nh-toast__icon { color: var(--success); }
.nh-toast--warning .nh-toast__icon { color: var(--warning); }
.nh-toast--error .nh-toast__icon { color: var(--error); }
.nh-toast--loading .nh-toast__icon { color: var(--text-secondary); }
.nh-toast__content { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 2px; }
.nh-toast__content strong { overflow: hidden; font-size: 12px; font-weight: 600; line-height: 17px; text-overflow: ellipsis; white-space: nowrap; }
.nh-toast__content span { overflow: hidden; color: var(--text-secondary); font-size: 11px; line-height: 15px; text-overflow: ellipsis; white-space: nowrap; }
.nh-toast__progress { position: absolute; right: 0; bottom: 0; left: 0; height: 2px; overflow: hidden; border-radius: 0 0 7px 7px; background: color-mix(in srgb, var(--text-secondary) 18%, transparent); }
.nh-toast__progress > span { display: block; height: 100%; transform-origin: left center; background: currentColor; transition: transform .05s linear; }
.nh-toast__spinner { animation: nh-toast-spin .8s linear infinite; }
@keyframes nh-toast-enter { from { opacity: 0; transform: translate(var(--toast-enter-x, 0), var(--toast-enter-y, -16px)) scale(.96); } to { opacity: 1; transform: translate(0, 0) scale(1); } }
.nh-toast--entering { animation: nh-toast-enter .45s cubic-bezier(.21,1.02,.55,1.15) both; }
@keyframes nh-toast-pulse { 0% { transform: scale(1); } 45% { transform: scale(1.025); } 100% { transform: scale(1); } }
.nh-toast--pulsing { animation: nh-toast-pulse .35s cubic-bezier(.21,1.02,.55,1.15) both; }
.nh-toast--shaking .nh-toast__content { animation: nh-toast-shake .4s ease both; }
.nh-toast--success .nh-toast__icon :deep(path:last-child) { stroke-dasharray: 16; stroke-dashoffset: 16; animation: nh-toast-check-draw .45s .12s ease forwards; }
@keyframes nh-toast-spin { to { transform: rotate(360deg); } }
@keyframes nh-toast-shake { 0%, 100% { transform: translateX(0); } 20% { transform: translateX(-6px); } 45% { transform: translateX(5px); } 70% { transform: translateX(-3px); } 90% { transform: translateX(2px); } }
@keyframes nh-toast-check-draw { to { stroke-dashoffset: 0; } }
@media (prefers-reduced-motion: reduce) { .nh-toast__spinner { animation: none; } .nh-toast--entering, .nh-toast--pulsing, .nh-toast--shaking .nh-toast__content { animation-duration: .01ms; } .nh-toast--success .nh-toast__icon :deep(path:last-child) { animation: none; stroke-dashoffset: 0; } .nh-toast__progress > span { transition: none; } }
</style>
