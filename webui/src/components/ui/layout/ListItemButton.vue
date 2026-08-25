<script setup lang="ts">
import { useAttrs } from "vue";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  disabled?: boolean;
  selected?: boolean;
  align?: "center" | "start";
  type?: "button" | "submit" | "reset";
  compact?: boolean;
}>(), {
  disabled: false,
  selected: false,
  align: "center",
  type: "button",
  compact: false,
});

const attrs = useAttrs();
</script>

<template>
  <li class="nh-list-item-button" :class="[`nh-list-item-button--align-${align}`, { 'nh-list-item-button--compact': compact, 'nh-list-item-button--selected': selected, 'nh-list-item-button--disabled': disabled }]" :data-selected="selected ? 'true' : undefined">
    <button v-bind="attrs" :type="type" :disabled="disabled">
      <span v-if="$slots.leading" class="nh-list-item-button__leading"><slot name="leading" /></span>
      <span class="nh-list-item-button__content"><slot /></span>
      <span v-if="$slots.trailing" class="nh-list-item-button__trailing"><slot name="trailing" /></span>
    </button>
  </li>
</template>

<style scoped>
.nh-list-item-button {
  --list-item-padding-block: 10px;
  --list-item-padding-inline: 0px;
  min-width: 0;
  position: relative;
}
.nh-list-item-button::after { position: absolute; right: var(--list-divider-inset, 0px); bottom: 0; left: var(--list-divider-inset, 0px); height: var(--list-divider-width, 0px); background: var(--list-divider-color, var(--border-divider)); content: ""; }
.nh-list-item-button:last-child::after { display: none; }
.nh-list-item-button > button { display: flex; box-sizing: border-box; width: 100%; min-height: 44px; min-width: 0; align-items: center; gap: 10px; padding: var(--list-item-padding-block) var(--list-item-padding-inline); border: 0; color: var(--text-primary); background: transparent; font: inherit; text-align: left; cursor: pointer; transition: background-color .14s ease, color .14s ease, box-shadow .14s ease; }
.nh-list-item-button > button:hover:not(:disabled) { background: var(--state-hover); }
.nh-list-item-button > button:active:not(:disabled) { background: var(--state-pressed); }
.nh-list-item-button--compact > button { min-height: 36px; padding: 8px 10px; border-radius: 5px; font-size: 12px; }
.nh-list-item-button--compact .nh-list-item-button__content { font-size: 12px; line-height: 18px; }
.nh-list-item-button > button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; border-radius: 4px; }
.nh-list-item-button--selected > button { background: color-mix(in srgb, var(--action-primary) 3%, transparent); }
.nh-list-item-button--align-start > button { align-items: flex-start; }
.nh-list-item-button--disabled { opacity: .62; }
.nh-list-item-button__leading, .nh-list-item-button__trailing { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; }
.nh-list-item-button__content { display: flex; min-width: 0; flex: 1 1 auto; flex-direction: column; gap: 3px; font-size: 13px; line-height: 1.35; }
.nh-list-item-button__content :deep(strong) { color: inherit; font-size: 13px; font-weight: 600; line-height: 1.35; }
.nh-list-item-button__content :deep(small), .nh-list-item-button__content :deep(p) { margin: 0; color: var(--text-secondary); font-size: 11px; line-height: 1.4; }
.nh-list-item-button__trailing { margin-left: auto; }
</style>
