<script setup lang="ts">
import { useAttrs } from "vue";

type ListItemElement = "li" | "div" | "article" | "section";
type ListItemAlign = "center" | "start";

defineOptions({ inheritAttrs: false });

withDefaults(defineProps<{
  as?: ListItemElement;
  align?: ListItemAlign;
  selected?: boolean;
  disabled?: boolean;
}>(), {
  as: "li",
  align: "center",
  selected: false,
  disabled: false,
});

const attrs = useAttrs();
</script>

<template>
  <component
    :is="as"
    v-bind="attrs"
    class="nh-list-item"
    :class="[`nh-list-item--align-${align}`, { 'nh-list-item--selected': selected, 'nh-list-item--disabled': disabled }]"
    :data-selected="selected ? 'true' : undefined"
    :aria-disabled="disabled ? 'true' : undefined"
  >
    <span v-if="$slots.leading" class="nh-list-item__leading"><slot name="leading" /></span>
    <span class="nh-list-item__content"><slot /></span>
    <span v-if="$slots.trailing" class="nh-list-item__trailing"><slot name="trailing" /></span>
  </component>
</template>

<style scoped>
.nh-list-item {
  --list-item-padding-block: 10px;
  --list-item-padding-inline: 0px;
  position: relative;
  display: flex;
  min-width: 0;
  min-height: 44px;
  align-items: center;
  gap: 10px;
  padding: var(--list-item-padding-block) var(--list-item-padding-inline);
  color: var(--text-primary);
  background: transparent;
}

.nh-list-item::after { position: absolute; right: var(--list-divider-inset, 0px); bottom: 0; left: var(--list-divider-inset, 0px); height: var(--list-divider-width, 0px); background: var(--list-divider-color, var(--border-divider)); content: ""; }
.nh-list-item:last-child::after { display: none; }
.nh-list-item--align-start { align-items: flex-start; }
.nh-list-item--selected { background: color-mix(in srgb, var(--action-primary) 3%, transparent); }
.nh-list-item--disabled { color: var(--text-disabled); opacity: .62; }
.nh-list-item__leading, .nh-list-item__trailing { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; }
.nh-list-item__content { display: flex; min-width: 0; flex: 1 1 auto; flex-direction: column; gap: 3px; }
.nh-list-item__content :deep(strong) { color: inherit; font-size: 13px; font-weight: 600; line-height: 1.35; }
.nh-list-item__content :deep(small), .nh-list-item__content :deep(p) { margin: 0; color: var(--text-secondary); font-size: 11px; line-height: 1.4; }
.nh-list-item__trailing { margin-left: auto; }
</style>
