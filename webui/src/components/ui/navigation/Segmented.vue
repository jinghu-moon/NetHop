<script setup lang="ts">
import { computed } from "vue";

export interface SegmentedOption {
  readonly value: string | number;
  readonly label: string;
  readonly disabled?: boolean;
}

const props = withDefaults(defineProps<{
  modelValue: string | number;
  options: readonly SegmentedOption[];
  disabled?: boolean;
}>(), { disabled: false });

const emit = defineEmits<{
  "update:modelValue": [value: string | number];
  change: [context: { value: string | number; selectedOption: SegmentedOption }];
}>();

const activeIndex = computed(() => props.options.findIndex((option) => Object.is(option.value, props.modelValue)));
const indicatorStyle = computed(() => ({
  width: `calc((100% - 4px) / ${Math.max(props.options.length, 1)})`,
  transform: `translateX(${Math.max(activeIndex.value, 0) * 100}%)`,
  opacity: activeIndex.value < 0 ? "0" : "1",
}));

function select(option: SegmentedOption): void {
  if (props.disabled || option.disabled || Object.is(option.value, props.modelValue)) return;
  emit("update:modelValue", option.value);
  emit("change", { value: option.value, selectedOption: option });
}
</script>

<template>
  <div class="nh-segmented segmented-control" :data-disabled="disabled">
    <span class="nh-segmented__indicator segmented-indicator" :style="indicatorStyle" aria-hidden="true" />
    <button v-for="option in options" :key="option.value" class="nh-segmented__item segmented-item" type="button" :data-active="Object.is(option.value, modelValue)" :disabled="disabled || option.disabled" @click="select(option)">{{ option.label }}</button>
  </div>
</template>

<style scoped>
.nh-segmented { position: relative; display: flex; width: 100%; min-height: 34px; box-sizing: border-box; padding: 2px; border-radius: 6px; background: var(--surface-muted); isolation: isolate; }
.nh-segmented__indicator { position: absolute; z-index: 0; top: 2px; bottom: 2px; left: 2px; border: 1px solid color-mix(in srgb, var(--border-default) 72%, transparent); border-radius: 5px; background: var(--surface); box-shadow: var(--shadow-1); pointer-events: none; transition: transform .35s cubic-bezier(.4,0,.2,1), opacity .16s ease; }
.nh-segmented__item { position: relative; z-index: 1; display: inline-flex; min-width: 0; min-height: 30px; align-items: center; justify-content: center; flex: 1 1 0; padding: 5px 8px; border: 0; border-radius: 5px; color: var(--text-secondary); background: transparent; font: inherit; font-size: 12px; font-weight: 500; line-height: 20px; white-space: nowrap; cursor: pointer; touch-action: manipulation; -webkit-tap-highlight-color: transparent; transition: color .16s ease; }
.nh-segmented__item[data-active="true"] { color: var(--text-primary); font-weight: 600; }
.nh-segmented__item:disabled { cursor: default; }
.nh-segmented[data-disabled="true"] { opacity: .48; }
.nh-segmented__item:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
@media (prefers-reduced-motion: reduce) { .nh-segmented__indicator, .nh-segmented__item { transition: none; } }
</style>
