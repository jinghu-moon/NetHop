<script setup lang="ts">
import { computed } from "vue";
import { IconCheck, IconChevronDown } from "@tabler/icons-vue";
import Dropdown from "@/components/ui/overlay/Dropdown.vue";
import List from "@/components/ui/layout/List.vue";
import Button from "@/components/ui/primitives/Button.vue";
import MenuItemRadio from "@/components/ui/menu/MenuItemRadio.vue";
import { ref } from "vue";

export interface ApplicationCategoryOption {
  readonly value: string;
  readonly label: string;
}

const props = defineProps<{ modelValue: string; options: readonly ApplicationCategoryOption[] }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const currentLabel = computed(() => props.options.find((option) => option.value === props.modelValue)?.label ?? "请选择");
const open = ref(false);

function select(value: string, close: () => void): void {
  emit("update:modelValue", value);
  close();
}
</script>

<template>
  <Dropdown v-model:open="open" class="application-category-dropdown" panel-class="application-category-menu" panel-width="148px" placement="bottom-start" :close-on-select="false">
    <template #trigger="{ open: isOpen }">
      <Button class="application-category-trigger" variant="outline" aria-haspopup="menu" :aria-expanded="isOpen" :data-open="isOpen">
        <span>{{ currentLabel }}</span>
        <IconChevronDown :size="17" />
      </Button>
    </template>
    <template #default="{ close }">
      <List class="application-category__options" spacing="none" aria-label="应用分类">
        <MenuItemRadio v-for="option in options" :key="option.value" :selected="option.value === modelValue" @click="select(option.value, close)">
          <template #suffix><IconCheck v-if="option.value === modelValue" :size="16" aria-hidden="true" /></template>
          {{ option.label }}
        </MenuItemRadio>
      </List>
    </template>
  </Dropdown>
</template>

<style scoped>
.application-category-dropdown {
  width: 126px;
  flex: 0 0 126px;
}

.application-category-trigger {
  display: flex;
  width: 100%;
  align-items: center;
  justify-content: space-between;
  min-height: 40px;
  padding: 0 10px 0 12px;
  border-radius: 6px;
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

.application-category-trigger[data-open="true"] svg {
  color: var(--nh-info);
  transform: rotate(180deg);
}
.application-category__options { min-width: 148px; }
.application-category__options :deep(.nh-menu-item-radio--selected) { color: var(--action-primary); background: color-mix(in srgb, var(--action-primary) 9%, transparent); }
.application-category__options :deep(.nh-menu-item-radio__suffix) { color: var(--action-primary); }
:global(.application-category-menu) { width: 148px; }
</style>
