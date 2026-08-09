<script setup lang="ts">
import { computed, ref } from "vue";
import { onClickOutside } from "@vueuse/core";
import { IconCheck, IconChevronDown } from "@tabler/icons-vue";

export interface ApplicationCategoryOption {
  readonly value: string;
  readonly label: string;
}

const props = defineProps<{ modelValue: string; options: readonly ApplicationCategoryOption[] }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const root = ref<HTMLElement>();
const open = ref(false);
const currentLabel = computed(() => props.options.find((option) => option.value === props.modelValue)?.label ?? "请选择");

onClickOutside(root, () => { open.value = false; });

function select(value: string): void {
  emit("update:modelValue", value);
  open.value = false;
}
</script>

<template>
  <div ref="root" class="application-category-dropdown" :data-open="open">
    <button class="application-category-trigger" type="button" @click="open = !open">
      <span>{{ currentLabel }}</span>
      <IconChevronDown :size="17" />
    </button>
    <Transition name="application-dropdown">
      <div v-if="open" class="application-category-menu">
        <button v-for="option in options" :key="option.value" type="button" :data-selected="option.value === modelValue" @click="select(option.value)">
          <span>{{ option.label }}</span>
          <IconCheck v-if="option.value === modelValue" :size="16" />
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.application-category-dropdown {
  position: relative;
  width: 126px;
  flex: 0 0 126px;
}

.application-category-dropdown[data-open="true"] {
  z-index: 40;
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

.application-category-dropdown[data-open="true"] .application-category-trigger {
  border-color: var(--focus-ring);
}

.application-category-dropdown[data-open="true"] .application-category-trigger svg {
  color: var(--nh-info);
  transform: rotate(180deg);
}

.application-category-menu {
  position: absolute;
  top: calc(100% + 5px);
  right: 0;
  width: 148px;
  overflow: hidden;
  padding: 4px;
  border: 1px solid var(--nh-border);
  border-radius: 6px;
  background: var(--nh-surface);
  box-shadow: var(--shadow-2);
  transform-origin: top right;
}

.application-category-menu button {
  display: flex;
  width: 100%;
  min-height: 36px;
  align-items: center;
  justify-content: space-between;
  padding: 0 9px;
  border: 0;
  border-radius: 4px;
  color: var(--nh-text);
  background: transparent;
  font-size: 12px;
  text-align: left;
}

.application-category-menu button:active,
.application-category-menu button[data-selected="true"] {
  color: var(--nh-selection);
  background: color-mix(in srgb, var(--nh-selection) 9%, var(--nh-surface));
}

.application-category-menu button svg {
  flex: 0 0 auto;
}

.application-dropdown-enter-active,
.application-dropdown-leave-active {
  transition: opacity .16s ease, transform .18s cubic-bezier(.2, .8, .2, 1);
}

.application-dropdown-enter-from,
.application-dropdown-leave-to {
  opacity: 0;
  transform: scale(.94) translateY(-5px);
}
</style>
