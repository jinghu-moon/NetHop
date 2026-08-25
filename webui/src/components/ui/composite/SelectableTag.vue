<script setup lang="ts">
import { computed, inject } from "vue";
import Tag from "../primitives/Tag.vue";
import Button from "../primitives/Button.vue";
import { tagGroupContextKey, type TagGroupContext, type TagGroupValue } from "./tagGroupContext";

type TagTone = "neutral" | "success" | "warning" | "danger" | "info";
type TagVariant = "soft" | "solid" | "outline";
type TagShape = "rounded" | "pill";
type TagSize = "s" | "m";

const props = withDefaults(defineProps<{
  value?: TagGroupValue;
  selected?: boolean;
  disabled?: boolean;
  tone?: TagTone;
  variant?: TagVariant;
  selectedVariant?: TagVariant;
  shape?: TagShape;
  size?: TagSize;
}>(), {
  selected: false,
  disabled: false,
  tone: "info",
  variant: "soft",
  selectedVariant: "solid",
  shape: "pill",
  size: "s",
});

const emit = defineEmits<{
  "update:selected": [value: boolean];
  click: [event: MouseEvent];
}>();

const group = inject<TagGroupContext | undefined>(tagGroupContextKey, undefined);
const isSelected = computed(() => group && props.value !== undefined ? group.isSelected(props.value) : props.selected);
const isDisabled = computed(() => props.disabled || group?.disabled.value === true);

function handleClick(event: MouseEvent): void {
  if (isDisabled.value) return;
  if (group && props.value !== undefined) {
    group.toggle(props.value);
  } else {
    emit("update:selected", !props.selected);
  }
  emit("click", event);
}
</script>

<template>
  <Button
    class="nh-selectable-tag"
    variant="text"
    size="s"
    :class="{ 'nh-selectable-tag--selected': isSelected }"
    type="button"
    :aria-pressed="isSelected"
    :disabled="isDisabled"
    @click="handleClick"
  >
    <Tag
      :tone="tone"
      :variant="isSelected ? selectedVariant : variant"
      :shape="shape"
      :size="size"
    >
      <template v-if="$slots.icon" #icon><slot name="icon" /></template>
      <slot />
    </Tag>
  </Button>
</template>

<style scoped>
:global(.nh-button.nh-selectable-tag) { display: inline-flex; min-width: 0; min-height: 20px; padding: 0; border: 0; border-radius: 999px; line-height: 1; overflow: visible; }
</style>
