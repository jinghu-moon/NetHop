<script setup lang="ts">
import { DropdownItem as TDropdownItem, DropdownMenu as TDropdownMenu } from "tdesign-mobile-vue";
import { computed } from "vue";

export interface OptionDropdownItem {
  readonly value: string;
  readonly label: string;
  readonly disabled?: boolean;
}

const props = withDefaults(defineProps<{
  modelValue: string;
  options: readonly OptionDropdownItem[];
  disabled?: boolean;
  compact?: boolean;
}>(), { disabled: false, compact: false });

const dropdownOptions = computed(() => props.options.map((option) => ({ ...option })));

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

function update(value: unknown): void {
  if (typeof value === "string") emit("update:modelValue", value);
}
</script>

<template>
  <div class="option-dropdown" :class="{ 'option-dropdown--compact': compact }">
    <TDropdownMenu :duration="120" :show-overlay="true">
      <TDropdownItem :value="modelValue" :options="dropdownOptions" :disabled="disabled" placement="right" @change="update" />
    </TDropdownMenu>
  </div>
</template>
