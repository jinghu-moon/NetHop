<script setup lang="ts">
import { ref } from "vue";
import { useOverlayRuntime } from "./useOverlayRuntime";
import Button from "../primitives/Button.vue";

export interface ActionSheetItem { readonly label: string; readonly disabled?: boolean; readonly color?: string; readonly icon?: unknown }
const props = withDefaults(defineProps<{ modelValue: boolean; items: readonly ActionSheetItem[]; title?: string; cancelLabel?: string }>(), { title: "", cancelLabel: "取消" });
const emit = defineEmits<{ "update:modelValue": [value: boolean]; selected: [item: ActionSheetItem, index: number]; cancel: []; close: [] }>();
const panel = ref<HTMLElement>();
function close(): void { emit("update:modelValue", false); emit("close"); }
function choose(item: ActionSheetItem, index: number): void { if (item.disabled) return; emit("selected", item, index); close(); }
function cancel(): void { emit("cancel"); close(); }
useOverlayRuntime(() => props.modelValue, close, panel, { type: "action-sheet" });
</script>
<template>
  <Teleport to="body"><Transition name="nh-popup"><div v-if="modelValue" class="nh-action-sheet" @mousedown.self="cancel"><section ref="panel" class="nh-action-sheet__panel" role="dialog" aria-modal="true" tabindex="-1"><h2 v-if="title">{{ title }}</h2><div class="nh-action-sheet__items"><button v-for="(item, index) in items" :key="item.label" type="button" :disabled="item.disabled" :style="{ color: item.color }" @click="choose(item, index)">{{ item.label }}</button></div><Button variant="outline" @click="cancel">{{ cancelLabel }}</Button></section></div></Transition></Teleport>
</template>
<style scoped>
.nh-action-sheet { position: fixed; z-index: 950; inset: 0; display: flex; align-items: flex-end; background: var(--scrim-default); }
.nh-action-sheet__panel { width: min(100%, 820px); max-height: 88dvh; overflow: auto; padding: 12px 16px max(16px, env(safe-area-inset-bottom)); border-radius: 8px 8px 0 0; background: var(--surface); box-shadow: var(--shadow-3); outline: 0; }
.nh-action-sheet h2 { margin: 4px 0 10px; overflow: hidden; color: var(--text-secondary); font-size: 12px; font-weight: 500; text-overflow: ellipsis; white-space: nowrap; }
.nh-action-sheet__items { display: grid; gap: 4px; }
.nh-action-sheet__items button { min-height: 44px; padding: 0 10px; border: 0; border-radius: 6px; color: var(--text-primary); background: transparent; font: inherit; text-align: left; }
.nh-action-sheet__items button:active:not(:disabled), .nh-action-sheet__items button:hover:not(:disabled) { background: var(--state-hover); }
.nh-action-sheet__items button:disabled { opacity: .45; }
.nh-action-sheet__panel > .nh-button { width: 100%; margin-top: 10px; }
</style>
