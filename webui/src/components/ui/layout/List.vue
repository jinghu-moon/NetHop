<script setup lang="ts">
import { useAttrs } from "vue";

type ListElement = "ul" | "ol";
type ListInset = "none" | "s" | "m";
type ListSpacing = "none" | "s" | "m";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  as?: ListElement;
  divided?: boolean;
  inset?: ListInset;
  spacing?: ListSpacing;
  ariaLabel?: string;
}>(), {
  as: "ul",
  divided: false,
  inset: "none",
  spacing: "s",
});

const attrs = useAttrs();
</script>

<template>
  <component
    :is="as"
    v-bind="attrs"
    class="nh-list"
    :class="[`nh-list--spacing-${spacing}`, `nh-list--inset-${inset}`, { 'nh-list--divided': divided }]"
    :aria-label="ariaLabel || attrs['aria-label']"
  >
    <slot />
  </component>
</template>

<style scoped>
.nh-list {
  --list-divider-color: var(--border-divider, var(--border-default));
  --list-divider-width: 0px;
  --list-divider-inset: 0px;
  --list-gap: 8px;
  display: flex;
  min-width: 0;
  margin: 0;
  padding: 0;
  flex-direction: column;
  list-style: none;
}

.nh-list--spacing-none { --list-gap: 0px; }
.nh-list--spacing-s { --list-gap: 8px; }
.nh-list--spacing-m { --list-gap: 12px; }
.nh-list--divided { --list-divider-width: 1px; gap: 0; }
.nh-list:not(.nh-list--divided) { gap: var(--list-gap); }
.nh-list--inset-s { --list-divider-inset: 8px; }
.nh-list--inset-m { --list-divider-inset: 16px; }
</style>
