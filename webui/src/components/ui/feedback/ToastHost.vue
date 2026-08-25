<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import Toast from "./Toast.vue";
import { useToast } from "./useToast";
import type { ToastAction, ToastItem, ToastPlacement } from "./toast-types";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  items?: readonly ToastItem[];
  placement?: ToastPlacement;
  maxVisible?: number;
}>(), { placement: "bottom-center", maxVisible: 3 });

const emit = defineEmits<{
  dismiss: [id: string];
  action: [action: ToastAction, toastId: string];
}>();

const store = useToast();
const progress = ref<Record<string, number>>({});
const entering = ref<Record<string, boolean>>({});
const pulsing = ref<Record<string, boolean>>({});
const shaking = ref<Record<string, boolean>>({});
interface ToastTimer { signature: string; frame?: number; started: number; remaining: number; duration: number; paused: boolean }
const timers = new Map<string, ToastTimer>();
const sourceItems = computed(() => props.items ?? store.items.value);
const visibleItems = computed(() => sourceItems.value.slice(-props.maxVisible));

function signature(item: ToastItem): string { return JSON.stringify([item.tone, item.message, item.detail, item.duration, item.persistent, item.showProgress, item.action, item.closable]); }
function stopTimer(id: string): void {
  const timer = timers.get(id);
  if (timer?.frame !== undefined) cancelAnimationFrame(timer.frame);
  timers.delete(id);
}
function dismiss(id: string): void {
  stopTimer(id);
  const nextProgress = { ...progress.value }; delete nextProgress[id]; progress.value = nextProgress;
  const nextEntering = { ...entering.value }; delete nextEntering[id]; entering.value = nextEntering;
  const nextPulsing = { ...pulsing.value }; delete nextPulsing[id]; pulsing.value = nextPulsing;
  const nextShaking = { ...shaking.value }; delete nextShaking[id]; shaking.value = nextShaking;
  if (props.items) emit("dismiss", id);
  else store.dismiss(id);
}
function startTimer(item: ToastItem): void {
  stopTimer(item.id);
  if (item.persistent || item.duration === null || item.duration === undefined || item.duration <= 0) return;
  const timer: ToastTimer = { signature: signature(item), started: performance.now(), remaining: item.duration, duration: item.duration, paused: false };
  timers.set(item.id, timer);
  progress.value = { ...progress.value, [item.id]: 1 };
  const tick = (now: number): void => {
    if (timers.get(item.id) !== timer) return;
    if (timer.paused) return;
    timer.remaining = Math.max(0, timer.remaining - (now - timer.started));
    timer.started = now;
    const ratio = Math.max(0, timer.remaining / timer.duration);
    progress.value = { ...progress.value, [item.id]: ratio };
    if (ratio <= 0) dismiss(item.id);
    else timer.frame = requestAnimationFrame(tick);
  };
  timer.frame = requestAnimationFrame(tick);
}

function pauseTimer(id: string): void {
  const item = sourceItems.value.find((candidate) => candidate.id === id);
  const timer = timers.get(id);
  if (!item?.pauseOnHover && item?.pauseOnHover !== undefined) return;
  if (!timer || timer.paused) return;
  timer.remaining = Math.max(0, timer.remaining - (performance.now() - timer.started));
  timer.paused = true;
  if (timer.frame !== undefined) cancelAnimationFrame(timer.frame);
}

function resumeTimer(id: string): void {
  const timer = timers.get(id);
  if (!timer || !timer.paused) return;
  if (timer.remaining <= 0) { dismiss(id); return; }
  timer.paused = false;
  timer.started = performance.now();
  timer.frame = requestAnimationFrame((now) => {
    const current = timers.get(id);
    if (current === timer) {
      timer.started = now;
      const ratio = Math.max(0, timer.remaining / timer.duration);
      progress.value = { ...progress.value, [id]: ratio };
      timer.frame = requestAnimationFrame(function tick(nextNow) {
        if (timers.get(id) !== timer || timer.paused) return;
        timer.remaining = Math.max(0, timer.remaining - (nextNow - timer.started));
        timer.started = nextNow;
        progress.value = { ...progress.value, [id]: timer.remaining / timer.duration };
        if (timer.remaining <= 0) dismiss(id);
        else timer.frame = requestAnimationFrame(tick);
      });
    }
  });
}

watch(sourceItems, (items) => {
  const active = new Set(items.map((item) => item.id));
  for (const id of timers.keys()) if (!active.has(id)) stopTimer(id);
  for (const item of items) {
    const current = timers.get(item.id);
    if (!current || current.signature !== signature(item)) {
      if (!current) entering.value = { ...entering.value, [item.id]: true };
      else {
        pulsing.value = { ...pulsing.value, [item.id]: false };
        requestAnimationFrame(() => {
          pulsing.value = { ...pulsing.value, [item.id]: true };
          window.setTimeout(() => { pulsing.value = { ...pulsing.value, [item.id]: false }; }, 380);
        });
      }
      if (item.tone === "error") {
        shaking.value = { ...shaking.value, [item.id]: false };
        requestAnimationFrame(() => {
          shaking.value = { ...shaking.value, [item.id]: true };
          window.setTimeout(() => { shaking.value = { ...shaking.value, [item.id]: false }; }, 430);
        });
      }
      startTimer(item);
    }
  }
}, { immediate: true, deep: true });

onBeforeUnmount(() => { for (const id of timers.keys()) stopTimer(id); });

function handleAction(action: ToastAction, toastId: string): void { emit("action", action, toastId); }
function handleClose(toastId: string): void { dismiss(toastId); }
</script>

<template>
  <div v-if="visibleItems.length" class="nh-toast-host" :class="`nh-toast-host--${placement}`" v-bind="$attrs" aria-label="操作提示">
    <TransitionGroup name="nh-toast" tag="div" class="nh-toast-host__stack">
      <Toast v-for="item in visibleItems" :key="item.id" :item="item" :progress="progress[item.id] ?? 1" :entering="entering[item.id] ?? false" :pulsing="pulsing[item.id] ?? false" :shaking="shaking[item.id] ?? false" @action="handleAction" @close="handleClose" @pause="pauseTimer" @resume="resumeTimer" />
    </TransitionGroup>
  </div>
</template>

<style scoped>
.nh-toast-host { --toast-enter-x: 0; --toast-enter-y: -16px; --toast-leave-x: 0; --toast-leave-y: -8px; position: fixed; z-index: var(--overlay-z-toast, 1100); right: max(12px, env(safe-area-inset-right)); left: max(12px, env(safe-area-inset-left)); display: flex; pointer-events: none; }
.nh-toast-host--top-center, .nh-toast-host--top-start, .nh-toast-host--top-end { top: max(12px, env(safe-area-inset-top)); }
.nh-toast-host--bottom-center, .nh-toast-host--bottom-start, .nh-toast-host--bottom-end { bottom: max(12px, calc(env(safe-area-inset-bottom) + 62px)); }
.nh-toast-host--top-center, .nh-toast-host--bottom-center { justify-content: center; }
.nh-toast-host--top-start, .nh-toast-host--bottom-start { justify-content: flex-start; }
.nh-toast-host--top-end, .nh-toast-host--bottom-end { justify-content: flex-end; }
.nh-toast-host--bottom-center, .nh-toast-host--bottom-start, .nh-toast-host--bottom-end { --toast-enter-y: 16px; --toast-leave-y: 8px; }
.nh-toast-host--top-start, .nh-toast-host--bottom-start { --toast-enter-x: -22px; --toast-leave-x: -22px; --toast-enter-y: 0; --toast-leave-y: 0; }
.nh-toast-host--top-end, .nh-toast-host--bottom-end { --toast-enter-x: 22px; --toast-leave-x: 22px; --toast-enter-y: 0; --toast-leave-y: 0; }
.nh-toast-host__stack { display: grid; width: min(100%, 420px); gap: 8px; }
.nh-toast-host :deep(.nh-toast) { pointer-events: auto; }
:global(.nh-toast-enter-active) { animation: nh-toast-enter .45s cubic-bezier(.21,1.02,.55,1.15) both; }
:global(.nh-toast-leave-active) { animation: nh-toast-leave .3s cubic-bezier(.22,.61,.36,1) both; }
:global(.nh-toast-move) { transition: transform .2s cubic-bezier(.16,1,.3,1); }
@keyframes nh-toast-enter { from { opacity: 0; transform: translate(var(--toast-enter-x), var(--toast-enter-y)) scale(.96); } to { opacity: 1; transform: translate(0, 0) scale(1); } }
@keyframes nh-toast-leave { from { opacity: 1; transform: translate(0, 0) scale(1); } to { opacity: 0; transform: translate(var(--toast-leave-x), var(--toast-leave-y)) scale(.95); } }
@media (prefers-reduced-motion: reduce) { :global(.nh-toast-enter-active), :global(.nh-toast-leave-active), :global(.nh-toast-move) { animation-duration: .01ms !important; transition: none; } }
</style>
