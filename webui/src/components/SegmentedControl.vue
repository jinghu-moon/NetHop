<script setup lang="ts">
import { computed } from "vue";

export interface SegmentedOption {
  readonly value: string | number;
  readonly label: string;
  readonly disabled?: boolean;
}

const props = withDefaults(defineProps<{
  readonly modelValue: string | number;
  readonly options: readonly SegmentedOption[];
  readonly disabled?: boolean;
}>(), {
  disabled: false,
});

const emit = defineEmits<{
  "update:modelValue": [value: string | number];
  change: [context: { value: string | number; selectedOption: SegmentedOption }];
}>();

const activeIndex = computed(() => props.options.findIndex((option) => Object.is(option.value, props.modelValue)));
const indicatorStyle = computed(() => {
  const count = Math.max(props.options.length, 1);
  const index = Math.max(activeIndex.value, 0);
  return {
    width: `calc((100% - 4px) / ${count})`,
    transform: `translateX(${index * 100}%)`,
    opacity: activeIndex.value < 0 ? "0" : "1",
  };
});

function select(option: SegmentedOption): void {
  if (props.disabled || option.disabled || Object.is(option.value, props.modelValue)) return;
  emit("update:modelValue", option.value);
  emit("change", { value: option.value, selectedOption: option });
}
</script>

<template>
  <div class="segmented-control" :data-disabled="disabled">
    <span class="segmented-indicator" :style="indicatorStyle"></span>
    <button
      v-for="option in options"
      :key="option.value"
      class="segmented-item"
      type="button"
      :data-active="Object.is(option.value, modelValue)"
      :disabled="disabled || option.disabled"
      @click="select(option)"
    >
      {{ option.label }}
    </button>
  </div>
</template>

<style scoped>
.segmented-control {
  position: relative;
  display: flex;
  width: 100%;
  min-height: 34px;
  padding: 2px;
  border-radius: 6px;
  background: var(--nh-bg);
  isolation: isolate;
}

.segmented-indicator {
  position: absolute;
  z-index: 0;
  top: 2px;
  bottom: 2px;
  left: 2px;
  border: 1px solid color-mix(in srgb, var(--nh-border) 72%, transparent);
  border-radius: 5px;
  background: var(--nh-surface);
  box-shadow: var(--shadow-1);
  pointer-events: none;
  transition: transform .35s cubic-bezier(.4, 0, .2, 1), opacity .2s ease, background-color .2s ease;
}

.segmented-item {
  position: relative;
  z-index: 1;
  display: inline-flex;
  min-width: 0;
  min-height: 30px;
  align-items: center;
  justify-content: center;
  flex: 1 1 0;
  padding: 5px 8px;
  border: 0;
  border-radius: 5px;
  color: var(--nh-muted);
  background: transparent;
  font-size: 12px;
  font-weight: 500;
  line-height: 20px;
  white-space: nowrap;
  cursor: pointer;
  transition: color .25s ease;
}

.segmented-item[data-active="true"] {
  color: var(--nh-text);
  font-weight: 600;
}

.segmented-item:disabled {
  cursor: default;
}

.segmented-control[data-disabled="true"] {
  opacity: .48;
}

.segmented-item {
    -webkit-tap-highlight-color: transparent !important;
}
</style>
