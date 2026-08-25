<script setup lang="ts">
export interface TabsItem { readonly value: string; readonly label: string; readonly disabled?: boolean }
defineProps<{ modelValue: string; items: readonly TabsItem[] }>();
const emit = defineEmits<{ "update:modelValue": [value: string]; change: [value: string] }>();
function select(value: string): void { emit("update:modelValue", value); emit("change", value); }
</script>
<template><div class="nh-tabs" role="tablist"><button v-for="item in items" :key="item.value" type="button" role="tab" :aria-selected="item.value === modelValue ? 'true' : 'false'" :tabindex="item.value === modelValue ? 0 : -1" :disabled="item.disabled" @click="select(item.value)">{{ item.label }}</button></div></template>
<style scoped>
.nh-tabs { display: grid; min-height: 40px; padding: 3px; border: 1px solid var(--border-default); border-radius: 7px; background: var(--surface-component); grid-auto-columns: minmax(0, 1fr); grid-auto-flow: column; gap: 3px; }
.nh-tabs button { min-width: 0; min-height: 32px; padding: 0 8px; border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; font: inherit; font-size: 12px; }
.nh-tabs button[aria-selected="true"] { color: var(--text-primary); background: var(--surface); box-shadow: var(--shadow-1); font-weight: 600; }
.nh-tabs button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
</style>
