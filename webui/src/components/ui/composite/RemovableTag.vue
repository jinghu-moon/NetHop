<script setup lang="ts">
import Tag from "../primitives/Tag.vue";
import IconButton from "../primitives/IconButton.vue";

type TagTone = "neutral" | "success" | "warning" | "danger" | "info";
type TagVariant = "soft" | "solid" | "outline";
type TagShape = "rounded" | "pill";
type TagSize = "s" | "m";

withDefaults(defineProps<{
  tone?: TagTone;
  variant?: TagVariant;
  shape?: TagShape;
  size?: TagSize;
  removeLabel?: string;
  disabled?: boolean;
}>(), {
  tone: "neutral",
  variant: "soft",
  shape: "rounded",
  size: "s",
  removeLabel: "移除标签",
  disabled: false,
});

const emit = defineEmits<{
  remove: [];
}>();
</script>

<template>
  <Tag
    class="nh-removable-tag"
    :class="{ 'nh-removable-tag--disabled': disabled }"
    :tone="tone"
    :variant="variant"
    :shape="shape"
    :size="size"
  >
    <template v-if="$slots.icon" #icon><slot name="icon" /></template>
    <slot />
    <template #end>
      <IconButton
        class="nh-removable-tag__remove"
        size="s"
        variant="text"
        :aria-label="removeLabel"
        :disabled="disabled"
        @click="emit('remove')"
      >
        <slot name="remove-icon" aria-hidden="true">
          <svg class="nh-removable-tag__icon" width="12" height="12" style="width: 12px !important; height: 12px !important" viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
        </slot>
      </IconButton>
    </template>
  </Tag>
</template>

<style scoped>
:deep(.nh-tag.nh-removable-tag) {
  --removable-tag-close-color: var(--text-secondary);
}

:deep(.nh-tag.nh-removable-tag .nh-tag__end) {
  margin-left: 2px;
}

:global(.nh-button.nh-removable-tag__remove) { width: 14px; min-width: 14px; height: 14px; min-height: 14px; padding: 1px; color: var(--removable-tag-close-color); }
:global(.nh-button.nh-removable-tag__remove:hover:not(:disabled)) { color: var(--error); }
:global(.nh-button.nh-removable-tag__remove svg) { width: 12px !important; height: 12px !important; }
:global(.nh-button.nh-removable-tag__remove .nh-button__label svg) { width: 12px !important; height: 12px !important; }
:global(.nh-button.nh-removable-tag__remove) .nh-button__label > svg { width: 12px !important; height: 12px !important; }
:global(.nh-removable-tag__icon) { width: 12px !important; height: 12px !important; }
:global(button.nh-removable-tag__remove svg) { width: 12px !important; height: 12px !important; }
:global(.nh-tag--m .nh-button.nh-removable-tag__remove) { width: 22px; min-width: 22px; height: 22px; min-height: 22px; }

.nh-removable-tag--disabled { opacity: .52; }
</style>
