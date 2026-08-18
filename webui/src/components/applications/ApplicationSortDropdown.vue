<script setup lang="ts">
import { computed } from "vue";
import { IconArrowsSort, IconCheck } from "@tabler/icons-vue";
import { Button as TButton, Switch as TSwitch } from "tdesign-mobile-vue";
import AnchoredDropdown from "@/components/AnchoredDropdown.vue";
import SegmentedControl from "@/components/SegmentedControl.vue";
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
  <AnchoredDropdown class="application-sort-dropdown" menu-label="应用排序" menu-class="application-sort-menu" menu-width="200px" :offset="6">
    <template #trigger="{ open, toggle }">
      <TButton
        class="application-sort-trigger"
        size="small"
        shape="square"
        variant="outline"
        theme="default"
        title="排序方式"
        aria-haspopup="menu"
        :aria-expanded="open"
        :data-open="open"
        @click="toggle"
      >
        <IconArrowsSort :size="19" />
      </TButton>
    </template>
    <template #default="{ close }">
      <section class="anchored-dropdown__section">
        <div class="anchored-dropdown__section-title">排序方式</div>
        <div class="anchored-dropdown__options" role="group" aria-label="排序字段">
          <button
            v-for="option in fields"
            :key="option.field"
            class="anchored-dropdown__option"
            type="button"
            role="menuitemradio"
            :aria-checked="option.field === modelValue.field"
            :data-selected="option.field === modelValue.field"
            @click="selectField(option.field, close)"
          >
            <span>{{ option.label }}</span>
            <IconCheck class="anchored-dropdown__option-icon" :size="18" aria-hidden="true" :data-visible="option.field === modelValue.field" />
          </button>
        </div>
      </section>
      <section class="anchored-dropdown__section anchored-dropdown__section--divided">
        <div class="anchored-dropdown__section-title">排序选项</div>
        <div class="anchored-dropdown__row">
          <span class="anchored-dropdown__row-label">方向</span>
          <div role="group" aria-label="排序方向">
            <SegmentedControl class="application-sort-direction" :model-value="modelValue.direction" :options="directionOptions" @change="updateDirection" />
          </div>
        </div>
        <div class="anchored-dropdown__row application-sort-priority">
          <span class="anchored-dropdown__row-label">已选优先</span>
          <TSwitch size="small" :value="selectedFirst" :disabled="!selectedFirstAvailable" aria-label="已选优先" @change="updateSelectedFirst" />
        </div>
        <p v-if="!selectedFirstAvailable" class="anchored-dropdown__hint">当前未选择应用，该选项暂不生效</p>
      </section>
    </template>
  </AnchoredDropdown>
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
</style>
