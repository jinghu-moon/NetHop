<script setup lang="ts">
import type { Component } from "vue";
export interface TabBarItem { readonly value: string; readonly label: string; readonly icon: Component }
defineProps<{ modelValue: string; items: readonly TabBarItem[] }>();
const emit = defineEmits<{ "update:modelValue": [value: string]; change: [value: string] }>();
function select(value: string): void { emit("update:modelValue", value); emit("change", value); }
</script>
<template><nav class="nh-tab-bar" aria-label="主导航"><button v-for="item in items" :key="item.value" type="button" :aria-current="item.value === modelValue ? 'page' : undefined" @click="select(item.value)"><span class="nh-tab-bar__icon"><component :is="item.icon" :size="21" stroke-width="1.8" aria-hidden="true" /></span><span>{{ item.label }}</span></button></nav></template>
<style scoped>
.nh-tab-bar { position: fixed; z-index: 100; right: 0; bottom: 0; left: 0; display: grid; min-height: calc(62px + env(safe-area-inset-bottom)); padding: 5px max(8px, calc((100vw - 820px) / 2)) env(safe-area-inset-bottom); border-top: 1px solid var(--border-divider); background: color-mix(in srgb, var(--surface) 94%, transparent); box-shadow: 0 -4px 16px rgb(0 0 0 / .05); grid-template-columns: repeat(4, minmax(0, 1fr)); backdrop-filter: blur(14px); }
.nh-tab-bar button { display: flex; min-width: 0; min-height: 52px; align-items: center; justify-content: center; padding: 3px 4px; border: 0; color: var(--text-secondary); background: transparent; flex-direction: column; gap: 2px; font-size: 10px; }
.nh-tab-bar button[aria-current="page"] { color: var(--text-primary); font-weight: 600; }
.nh-tab-bar__icon { display: inline-flex; width: 42px; height: 28px; align-items: center; justify-content: center; border-radius: 14px; }
.nh-tab-bar button[aria-current="page"] .nh-tab-bar__icon { color: var(--nh-selection-text); background: var(--nh-selection); }
.nh-tab-bar button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
</style>
