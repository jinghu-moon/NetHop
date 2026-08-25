<script setup lang="ts">
import { ref, useAttrs } from "vue";

defineOptions({ inheritAttrs: false });
const attrs = useAttrs();
const root = ref<HTMLElement>();
const scroll = ref<HTMLElement>();

defineExpose({ root, scroll });
</script>

<template>
  <div ref="root" v-bind="attrs" class="nh-dropdown-panel" data-dropdown-panel="true">
    <slot name="overlay" />
    <div ref="scroll" class="nh-dropdown-panel__scroll"><slot /></div>
  </div>
</template>

<style scoped>
.nh-dropdown-panel {
  position: fixed;
  box-sizing: border-box;
  width: max-content;
  min-width: 176px;
  max-width: min(320px, calc(100vw - 20px));
  overflow: visible;
  border: 1px solid var(--border-default);
  border-radius: 8px;
  color: var(--text-primary);
  background: var(--surface);
  box-shadow: var(--shadow-2);
  pointer-events: auto;
}
.nh-dropdown-panel__scroll {
  max-height: inherit;
  overflow-x: clip;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 6px;
  scrollbar-width: thin;
}
</style>
