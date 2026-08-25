<script setup lang="ts">
import { useAttrs } from "vue";

defineOptions({ inheritAttrs: false });

export interface SelectOption {
  readonly value: string;
  readonly label: string;
  readonly disabled?: boolean;
}

withDefaults(defineProps<{
  modelValue?: string;
  options: readonly SelectOption[];
  disabled?: boolean;
  required?: boolean;
  id?: string;
  ariaLabel?: string;
}>(), { modelValue: "", disabled: false, required: false, id: "", ariaLabel: "" });

const attrs = useAttrs();
const emit = defineEmits<{ "update:modelValue": [value: string]; change: [value: string, event: Event] }>();
</script>

<template>
  <span class="nh-select">
    <select
      v-bind="attrs"
      :id="id || undefined"
      :value="modelValue"
      :disabled="disabled"
      :required="required"
      :aria-label="ariaLabel || (typeof attrs['aria-label'] === 'string' ? attrs['aria-label'] : undefined)"
      @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value); emit('change', ($event.target as HTMLSelectElement).value, $event)"
    >
      <option v-for="option in options" :key="option.value" :value="option.value" :disabled="option.disabled">{{ option.label }}</option>
    </select>
  </span>
</template>

<style scoped>
.nh-select { display: inline-flex; min-width: 0; max-width: 100%; }
.nh-select select { box-sizing: border-box; width: 100%; min-height: 36px; min-width: 0; padding: 0 28px 0 10px; border: 1px solid var(--border-default); border-radius: 6px; color: var(--text-primary); background: var(--surface); font: inherit; font-size: 12px; outline: none; }
.nh-select select:focus-visible { border-color: var(--focus-ring); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus-ring) 18%, transparent); }
.nh-select select:disabled { cursor: default; opacity: .52; }
</style>
