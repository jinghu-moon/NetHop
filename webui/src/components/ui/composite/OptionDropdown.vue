<script setup lang="ts">
import { computed, ref } from "vue";
import { IconCheck, IconChevronDown } from "@tabler/icons-vue";
import Dropdown from "@/components/ui/overlay/Dropdown.vue";
import MenuList from "@/components/ui/menu/MenuList.vue";
import MenuItem from "@/components/ui/menu/MenuItem.vue";
import Button from "@/components/ui/primitives/Button.vue";

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
  ariaLabel?: string;
}>(), { disabled: false, compact: false, ariaLabel: "" });

const selectedLabel = computed(() => props.options.find((option) => option.value === props.modelValue)?.label ?? props.modelValue);
const open = ref(false);
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

function select(value: string | undefined, close: () => void): void {
  if (value === undefined) return;
  emit("update:modelValue", value);
  close();
}
</script>

<template>
  <Dropdown v-model:open="open" class="option-dropdown" :class="{ 'option-dropdown--compact': compact }" placement="bottom-start" :disabled="disabled">
    <template #trigger="{ open: isOpen }">
      <Button class="option-dropdown__trigger" variant="outline" :size="compact ? 's' : 'm'" role="combobox" :aria-expanded="isOpen ? 'true' : 'false'" :aria-label="ariaLabel || selectedLabel" :disabled="disabled">
        <span class="option-dropdown__label">{{ selectedLabel }}</span><IconChevronDown aria-hidden="true" />
      </Button>
    </template>
    <template #default="{ close }">
      <MenuList semantic="listbox" class="option-dropdown__options" :model-value="modelValue" aria-label="选项" @select="(value) => select(value, close)">
        <MenuItem v-for="option in options" :key="option.value" :value="option.value" :disabled="option.disabled === true">
          {{ option.label }}
          <template #suffix><IconCheck v-if="option.value === modelValue" :size="15" aria-hidden="true" /></template>
        </MenuItem>
      </MenuList>
    </template>
  </Dropdown>
</template>

<style scoped>
.option-dropdown { min-width: 0; }
.option-dropdown__trigger { display: flex; width: 100%; min-height: 40px; align-items: center; justify-content: space-between; gap: 8px; padding: 0 9px; border-radius: 6px; font-size: 12px; text-align: left; }
.option-dropdown__label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.option-dropdown__trigger svg { width: 15px; height: 15px; flex: 0 0 auto; color: var(--text-secondary); }
.option-dropdown__options { min-width: 150px; }
.option-dropdown__options :deep(.nh-menu-item--selected) { color: var(--action-primary); background: color-mix(in srgb, var(--action-primary) 9%, transparent); }
.option-dropdown__options :deep(.nh-menu-item__suffix) { color: var(--action-primary); }
.option-dropdown--compact .option-dropdown__trigger { min-height: 36px; font-size: 11px; }
</style>
