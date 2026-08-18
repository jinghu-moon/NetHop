<script setup lang="ts">
import { computed } from "vue";
import { IconCheck, IconChevronDown } from "@tabler/icons-vue";
import AnchoredDropdown from "@/components/AnchoredDropdown.vue";

export interface ApplicationCategoryOption {
  readonly value: string;
  readonly label: string;
}

const props = defineProps<{ modelValue: string; options: readonly ApplicationCategoryOption[] }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const currentLabel = computed(() => props.options.find((option) => option.value === props.modelValue)?.label ?? "请选择");

function select(value: string, close: () => void): void {
  emit("update:modelValue", value);
  close();
}
</script>

<template>
  <AnchoredDropdown class="application-category-dropdown" menu-label="应用分类" menu-class="application-category-menu" menu-width="148px" :offset="5">
    <template #trigger="{ open, toggle }">
      <button class="application-category-trigger" type="button" aria-haspopup="menu" :aria-expanded="open" :data-open="open" @click="toggle">
        <span>{{ currentLabel }}</span>
        <IconChevronDown :size="17" />
      </button>
    </template>
    <template #default="{ close }">
      <div class="anchored-dropdown__options">
        <button v-for="option in options" :key="option.value" class="anchored-dropdown__option" type="button" role="menuitemradio" :aria-checked="option.value === modelValue" :data-selected="option.value === modelValue" @click="select(option.value, close)">
          <span>{{ option.label }}</span>
          <IconCheck v-if="option.value === modelValue" class="anchored-dropdown__option-icon" :size="16" />
        </button>
      </div>
    </template>
  </AnchoredDropdown>
</template>

<style scoped>
.application-category-dropdown {
  width: 126px;
  flex: 0 0 126px;
}

.application-category-trigger {
  display: flex;
  width: 100%;
  min-height: 40px;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px 0 12px;
  border: 1px solid var(--nh-border);
  border-radius: 6px;
  color: var(--nh-text);
  background: var(--nh-surface);
  gap: 8px;
}

.application-category-trigger span {
  overflow: hidden;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.application-category-trigger svg {
  flex: 0 0 auto;
  color: var(--nh-muted);
  transition: transform .2s cubic-bezier(.4, 0, .2, 1), color .2s ease;
}

.application-category-trigger[data-open="true"] {
  border-color: var(--focus-ring);
}

.application-category-trigger[data-open="true"] svg {
  color: var(--nh-info);
  transform: rotate(180deg);
}
</style>
