<script setup lang="ts">
import { ref } from "vue";
import { useOverlayRuntime } from "./useOverlayRuntime";

const props = withDefaults(defineProps<{ modelValue: boolean; closeOnBackdrop?: boolean; destroyOnClose?: boolean }>(), { closeOnBackdrop: true, destroyOnClose: true });
const emit = defineEmits<{ "update:modelValue": [value: boolean]; visibleChange: [value: boolean] }>();
const panel = ref<HTMLElement>();
function close(): void { emit("update:modelValue", false); emit("visibleChange", false); }
function backdrop(): void { if (props.closeOnBackdrop) close(); }
useOverlayRuntime(() => props.modelValue, close, panel, { type: "popup" });
</script>

<template>
  <Teleport to="body">
    <Transition name="nh-popup">
      <div v-if="modelValue" class="nh-popup" @mousedown.self="backdrop">
        <section ref="panel" class="nh-popup__panel" role="dialog" aria-modal="true" tabindex="-1"><slot /></section>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.nh-popup { position: fixed; z-index: 950; inset: 0; display: flex; align-items: flex-end; justify-content: center; background: var(--scrim-default); }
.nh-popup__panel { width: min(100%, 820px); max-height: min(88dvh, var(--nh-visual-height, 88dvh)); overflow: auto; border-radius: 8px 8px 0 0; outline: 0; color: var(--text-primary); background: var(--surface); box-shadow: var(--shadow-3); }
.nh-popup-enter-active, .nh-popup-leave-active { transition: opacity .16s ease; }
.nh-popup-enter-active .nh-popup__panel, .nh-popup-leave-active .nh-popup__panel { transition: transform .16s cubic-bezier(.4, 0, .2, 1); }
.nh-popup-enter-from, .nh-popup-leave-to { opacity: 0; }
.nh-popup-enter-from .nh-popup__panel, .nh-popup-leave-to .nh-popup__panel { transform: translateY(100%); }
@media (prefers-reduced-motion: reduce) { .nh-popup, .nh-popup__panel { transition: none !important; } }
</style>
