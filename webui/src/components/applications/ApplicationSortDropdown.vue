<script setup lang="ts">
import { computed } from "vue";
import { IconArrowsSort, IconCheck } from "@tabler/icons-vue";
import { ref } from "vue";
import Dropdown from "@/components/ui/overlay/Dropdown.vue";
import List from "@/components/ui/layout/List.vue";
import MenuItemRadio from "@/components/ui/menu/MenuItemRadio.vue";
import Segmented from "@/components/ui/navigation/Segmented.vue";
import Switch from "@/components/ui/primitives/Switch.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";
import type { ApplicationSort, ApplicationSortDirection, ApplicationSortField } from "@/model/application-sort";

export interface ApplicationSortFieldOption {
  readonly field: ApplicationSortField;
  readonly label: string;
}

const props = defineProps<{
  modelValue: ApplicationSort;
  fields: readonly ApplicationSortFieldOption[];
  selectedFirst: boolean;
  selectedCount: number;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: ApplicationSort];
  "selectedFirstChange": [value: boolean];
}>();
const selectedFirstAvailable = computed(() => props.selectedCount > 0);
const open = ref(false);
const directionOptions = [
  { value: "asc", label: "升序" },
  { value: "desc", label: "降序" },
] as const;

function updateDirection(context: { value: string | number }): void {
  if (context.value !== "asc" && context.value !== "desc") return;
  const direction: ApplicationSortDirection = context.value;
  emit("update:modelValue", { ...props.modelValue, direction });
}

function selectField(field: ApplicationSortField, close: () => void): void {
  emit("update:modelValue", { ...props.modelValue, field });
  close();
}

function updateSelectedFirst(value: unknown): void {
  if (!selectedFirstAvailable.value) return;
  emit("selectedFirstChange", value === true || value === "true");
}
</script>

<template>
  <Dropdown v-model:open="open" class="application-sort-dropdown" panel-class="application-sort-menu" panel-width="200px" placement="bottom-end" :close-on-select="false">
    <template #trigger="{ open: isOpen }">
      <IconButton
        class="application-sort-trigger"
        size="s"
        variant="outline"
        aria-label="排序方式"
        title="排序方式"
        aria-haspopup="menu"
        :aria-expanded="isOpen"
        :data-open="isOpen"
      >
        <IconArrowsSort :size="19" />
      </IconButton>
    </template>
    <template #default="{ close }">
      <section class="application-sort-section">
        <div class="application-sort-section__title">排序方式</div>
        <List spacing="none" aria-label="排序字段">
          <MenuItemRadio v-for="option in fields" :key="option.field" :selected="option.field === modelValue.field" @click="selectField(option.field, close)">
            <template #suffix><IconCheck v-if="option.field === modelValue.field" :size="18" aria-hidden="true" /></template>
            {{ option.label }}
          </MenuItemRadio>
        </List>
      </section>
      <section class="application-sort-section application-sort-section--divided">
        <div class="application-sort-section__title">排序选项</div>
        <div class="application-sort-row">
          <span>方向</span>
          <div role="group" aria-label="排序方向">
            <Segmented class="application-sort-direction" :model-value="modelValue.direction" :options="directionOptions" @change="updateDirection" />
          </div>
        </div>
        <div class="application-sort-row application-sort-priority">
          <span>已选优先</span>
          <Switch size="s" :model-value="selectedFirst" :disabled="!selectedFirstAvailable" aria-label="已选优先" @change="updateSelectedFirst" />
        </div>
        <p v-if="!selectedFirstAvailable" class="application-sort-hint">当前未选择应用，该选项暂不生效</p>
      </section>
    </template>
  </Dropdown>
</template>

<style scoped>
.application-sort-dropdown {
  flex: 0 0 auto;
}

.application-sort-trigger[data-open="true"] {
  border-color: var(--focus-ring);
  color: var(--nh-info);
}

.application-sort-direction {
  width: 130px;
}
.application-sort-section { box-sizing: border-box; width: 100%; min-width: 0; padding: 6px; }
.application-sort-section--divided { border-top: 1px solid var(--border-divider); }
.application-sort-section__title { padding: 3px 4px 6px; color: var(--text-secondary); font-size: 11px; }
.application-sort-section :deep(.nh-menu-item-radio--selected) { color: var(--action-primary); background: color-mix(in srgb, var(--action-primary) 9%, transparent); }
.application-sort-section :deep(.nh-menu-item-radio__suffix) { color: var(--action-primary); }
.application-sort-row { display: flex; min-height: 44px; align-items: center; justify-content: space-between; gap: 12px; padding: 4px; font-size: 12px; }
.application-sort-hint { margin: 0 4px 3px; color: var(--text-secondary); font-size: 11px; line-height: 18px; }
:global(.application-sort-menu) { width: 200px; }
</style>
