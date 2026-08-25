<script setup lang="ts">
import { computed, provide } from "vue";
import { tagGroupContextKey, type TagGroupContext, type TagGroupValue } from "./tagGroupContext";

const props = withDefaults(defineProps<{
  modelValue?: TagGroupValue[];
  disabled?: boolean;
}>(), {
  modelValue: () => [],
  disabled: false,
});

const emit = defineEmits<{
  "update:modelValue": [value: TagGroupValue[]];
}>();

const disabled = computed(() => props.disabled);

function isSelected(value: TagGroupValue): boolean {
  return props.modelValue.includes(value);
}

function toggle(value: TagGroupValue): void {
  if (props.disabled) return;
  const next = isSelected(value)
    ? props.modelValue.filter((item) => item !== value)
    : [...props.modelValue, value];
  emit("update:modelValue", next);
}

provide(tagGroupContextKey, {
  isSelected,
  toggle,
  disabled,
} satisfies TagGroupContext);
</script>

<template>
  <div class="nh-tag-group" role="group">
    <slot />
  </div>
</template>

<style scoped>
.nh-tag-group {
  display: inline-flex;
  max-width: 100%;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
</style>
