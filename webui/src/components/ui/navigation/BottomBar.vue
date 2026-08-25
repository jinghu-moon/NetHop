<script setup lang="ts">
import { computed, useAttrs, type Component } from "vue";

export type BottomBarBadge = number | string | "dot";
export type BottomBarIndicator = "pill" | "line" | "none";
export type BottomBarVariant = "standard" | "floating";

export interface BottomBarItem {
  readonly value: string;
  readonly label: string;
  readonly icon?: Component;
  readonly activeIcon?: Component;
  readonly badge?: BottomBarBadge;
  readonly ariaLabel?: string;
}

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue: string;
  items: readonly BottomBarItem[];
  hidden?: boolean;
  fixed?: boolean;
  placeholder?: boolean;
  safeAreaInsetBottom?: boolean;
  showLabel?: boolean;
  indicator?: BottomBarIndicator;
  variant?: BottomBarVariant;
  bordered?: boolean;
  showItemDivider?: boolean;
  ariaLabel?: string;
}>(), {
  hidden: false,
  fixed: true,
  placeholder: false,
  safeAreaInsetBottom: true,
  showLabel: true,
  indicator: "pill",
  variant: "standard",
  bordered: true,
  showItemDivider: false,
  ariaLabel: "底部导航",
});

const attrs = useAttrs();
const emit = defineEmits<{
  "update:modelValue": [value: string];
  change: [value: string];
  reselect: [value: string];
}>();

const itemCount = computed(() => Math.max(props.items.length, 1));
const rootClasses = computed(() => [
  `nh-bottom-bar--${props.variant}`,
  `nh-bottom-bar--indicator-${props.indicator}`,
  { "nh-bottom-bar--fixed": props.fixed, "nh-bottom-bar--hidden": props.hidden, "nh-bottom-bar--safe": props.safeAreaInsetBottom, "nh-bottom-bar--bordered": props.bordered, "nh-bottom-bar--item-divider": props.showItemDivider },
]);

function select(item: BottomBarItem): void {
  if (item.value === props.modelValue) emit("reselect", item.value);
  else {
    emit("update:modelValue", item.value);
    emit("change", item.value);
  }
}
</script>

<template>
  <nav
    v-if="!hidden || placeholder"
    v-bind="attrs"
    class="nh-bottom-bar"
    :class="rootClasses"
    :aria-label="ariaLabel"
    :aria-hidden="hidden ? 'true' : undefined"
    :style="{ '--bottom-bar-item-count': itemCount }"
  >
    <button
      v-for="item in items"
      :key="item.value"
      type="button"
      class="nh-bottom-bar__item"
      :class="{ 'nh-bottom-bar__item--active': item.value === modelValue, 'nh-bottom-bar__item--icon-only': !showLabel, 'nh-bottom-bar__item--text-only': !item.icon && !item.activeIcon }"
      :aria-current="item.value === modelValue ? 'page' : undefined"
      :aria-label="item.ariaLabel || item.label"
      @click="select(item)"
    >
      <span v-if="item.icon || item.activeIcon" class="nh-bottom-bar__icon" :class="{ 'nh-bottom-bar__icon--indicator': indicator !== 'none' }">
        <component :is="item.value === modelValue && item.activeIcon ? item.activeIcon : item.icon" :size="22" stroke-width="1.8" aria-hidden="true" />
        <span v-if="item.badge !== undefined" class="nh-bottom-bar__badge" :class="{ 'nh-bottom-bar__badge--dot': item.badge === 'dot' }" :aria-label="typeof item.badge === 'string' && item.badge !== 'dot' ? item.badge : undefined">
          <template v-if="item.badge !== 'dot'">{{ item.badge }}</template>
        </span>
      </span>
      <span v-if="showLabel" class="nh-bottom-bar__label">
        {{ item.label }}
        <span v-if="item.badge !== undefined && !item.icon && !item.activeIcon" class="nh-bottom-bar__badge" :class="{ 'nh-bottom-bar__badge--dot': item.badge === 'dot' }">
          <template v-if="item.badge !== 'dot'">{{ item.badge }}</template>
        </span>
      </span>
    </button>
  </nav>
  <div v-if="placeholder && fixed && !hidden" class="nh-bottom-bar__placeholder" :class="{ 'nh-bottom-bar__placeholder--safe': safeAreaInsetBottom }" aria-hidden="true" />
</template>

<style scoped>
.nh-bottom-bar {
  --bottom-bar-height: 62px;
  --bottom-bar-safe: env(safe-area-inset-bottom, 0px);
  --bottom-bar-surface: color-mix(in srgb, var(--surface) 94%, transparent);
  --bottom-bar-active: var(--nh-selection, var(--action-primary));
  position: relative;
  z-index: var(--overlay-z-bottom-bar, 100);
  display: grid;
  width: 100%;
  min-height: var(--bottom-bar-height);
  box-sizing: border-box;
  padding: 5px 8px 0;
  border-top: 0 solid var(--border-divider);
  color: var(--text-secondary);
  background: var(--bottom-bar-surface);
  grid-template-columns: repeat(var(--bottom-bar-item-count), minmax(0, 1fr));
  backdrop-filter: blur(14px);
  transition: opacity .16s ease, transform .18s ease, box-shadow .18s ease;
}
.nh-bottom-bar--bordered { border-top-width: 1px; }
.nh-bottom-bar--fixed { position: fixed; right: 0; bottom: 0; left: 0; padding-right: max(8px, calc((100vw - 820px) / 2)); padding-left: max(8px, calc((100vw - 820px) / 2)); }
.nh-bottom-bar--safe { min-height: calc(var(--bottom-bar-height) + var(--bottom-bar-safe)); padding-bottom: var(--bottom-bar-safe); }
.nh-bottom-bar--floating { width: calc(100% - 24px); margin-inline: 12px; border: 0 solid var(--border-default); border-radius: 18px; box-shadow: var(--shadow-2); }
.nh-bottom-bar--floating.nh-bottom-bar--bordered { border-width: 1px; }
.nh-bottom-bar--floating.nh-bottom-bar--fixed { right: 12px; bottom: max(12px, calc(var(--bottom-bar-safe) + 8px)); left: 12px; width: auto; margin-inline: 0; }
.nh-bottom-bar--floating.nh-bottom-bar--safe { min-height: var(--bottom-bar-height); padding-bottom: 0; }
.nh-bottom-bar--hidden { visibility: hidden; opacity: 0; pointer-events: none; transform: translateY(8px); }
.nh-bottom-bar__item { position: relative; display: flex; min-width: 0; min-height: 52px; align-items: center; justify-content: center; padding: 3px 4px; border: 0; color: inherit; background: transparent; font: inherit; font-size: 10px; line-height: 1.1; flex-direction: column; gap: 2px; cursor: pointer; -webkit-tap-highlight-color: transparent; }
.nh-bottom-bar__item:active { transform: scale(.97); }
.nh-bottom-bar__item:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; border-radius: 8px; }
.nh-bottom-bar--item-divider .nh-bottom-bar__item:not(:last-child)::after { position: absolute; top: 13px; right: 0; bottom: 13px; width: 1px; background: var(--border-divider); content: ""; }
.nh-bottom-bar__icon { position: relative; display: inline-flex; width: 42px; height: 28px; align-items: center; justify-content: center; border-radius: 14px; color: currentColor; transition: color .16s ease, background-color .18s ease, transform .22s cubic-bezier(.34,1.3,.64,1); }
.nh-bottom-bar__icon :deep(svg) { transition: transform .22s cubic-bezier(.34,1.3,.64,1), stroke-width .16s ease; }
.nh-bottom-bar__item--active { color: var(--text-primary); font-weight: 600; }
.nh-bottom-bar__item--active .nh-bottom-bar__icon--indicator { color: var(--nh-selection-text, var(--action-primary)); background: var(--bottom-bar-active); }
.nh-bottom-bar__item--active .nh-bottom-bar__icon { transform: translateY(-1px); }
.nh-bottom-bar__item--active .nh-bottom-bar__icon :deep(svg) { transform: scale(1.08); stroke-width: 2; }
.nh-bottom-bar--indicator-line .nh-bottom-bar__item--active .nh-bottom-bar__icon { background: transparent; color: var(--action-primary); }
.nh-bottom-bar--indicator-line .nh-bottom-bar__item--active::before { position: absolute; top: -5px; width: 22px; height: 3px; border-radius: 0 0 3px 3px; background: var(--action-primary); content: ""; }
.nh-bottom-bar--indicator-none .nh-bottom-bar__item--active .nh-bottom-bar__icon { background: transparent; color: var(--action-primary); }
.nh-bottom-bar__item--icon-only { gap: 0; }
.nh-bottom-bar__item--icon-only .nh-bottom-bar__icon { height: 36px; }
.nh-bottom-bar__item--text-only { min-height: 52px; font-size: 13px; }
.nh-bottom-bar__item--text-only .nh-bottom-bar__label { position: relative; display: inline-flex; min-width: 64px; min-height: 36px; align-items: center; justify-content: center; padding: 0 12px; border-radius: 18px; }
.nh-bottom-bar--indicator-pill .nh-bottom-bar__item--text-only.nh-bottom-bar__item--active .nh-bottom-bar__label { color: var(--nh-selection-text, var(--action-primary)); background: var(--bottom-bar-active); }
.nh-bottom-bar__badge { position: absolute; top: -5px; right: -7px; min-width: 16px; height: 16px; box-sizing: border-box; padding: 0 4px; border-radius: 9px; color: var(--text-inverse); background: var(--error); font-size: 9px; font-weight: 700; line-height: 16px; text-align: center; }
.nh-bottom-bar__badge--dot { width: 9px; min-width: 9px; height: 9px; padding: 0; top: -2px; right: -3px; border-radius: 50%; }
.nh-bottom-bar__placeholder { min-height: var(--bottom-bar-height); }
.nh-bottom-bar__placeholder--safe { min-height: calc(var(--bottom-bar-height) + env(safe-area-inset-bottom, 0px)); }
@media (prefers-reduced-motion: reduce) { .nh-bottom-bar, .nh-bottom-bar__icon, .nh-bottom-bar__icon :deep(svg) { transition: none; } .nh-bottom-bar__item:active { transform: none; } }
</style>
