<script setup lang="ts">
import { computed, inject, useAttrs } from "vue";
import { menuContextKey } from "./menu-context";
import Button from "../primitives/Button.vue";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  value?: string;
  description?: string;
  disabled?: boolean;
  danger?: boolean;
  divider?: boolean;
}>(), {
  description: "",
  disabled: false,
  danger: false,
  divider: false,
});

const attrs = useAttrs();
const menu = inject(menuContextKey);
const selected = computed(() => props.value !== undefined && menu?.modelValue.value === props.value);
const isDisabled = computed(() => props.disabled || menu?.disabled.value === true);

function select(event: Event): void {
  if (props.divider || isDisabled.value) return;
  menu?.select(props.value, event);
}
</script>

<template>
  <div v-if="divider" class="nh-menu-item__divider" role="separator" data-menu-item="true" data-divider="true" aria-hidden="true"></div>
  <Button
    v-if="menu?.semantic.value === 'menu'"
    v-bind="attrs"
    class="nh-menu-item"
    :class="{ 'nh-menu-item--selected': selected, 'nh-menu-item--disabled': isDisabled, 'nh-menu-item--danger': danger }"
    variant="text"
    size="s"
    data-menu-item="true"
    :data-value="value"
    role="menuitem"
    :aria-disabled="isDisabled ? 'true' : undefined"
    :disabled="isDisabled"
    @click="select"
  >
    <span v-if="$slots.prefix" class="nh-menu-item__prefix"><slot name="prefix" /></span>
    <span class="nh-menu-item__content"><strong><slot /></strong><small v-if="description">{{ description }}</small></span>
    <span v-if="$slots.suffix" class="nh-menu-item__suffix"><slot name="suffix" /></span>
  </Button>
  <div
    v-else
    v-bind="attrs"
    class="nh-menu-item"
    :class="{ 'nh-menu-item--selected': selected, 'nh-menu-item--disabled': isDisabled, 'nh-menu-item--danger': danger }"
    data-menu-item="true"
    :data-value="value"
    role="option"
    :aria-selected="selected ? 'true' : 'false'"
    :aria-disabled="isDisabled ? 'true' : undefined"
    :tabindex="isDisabled ? -1 : 0"
    @click="select"
  >
    <span v-if="$slots.prefix" class="nh-menu-item__prefix"><slot name="prefix" /></span>
    <span class="nh-menu-item__content"><strong><slot /></strong><small v-if="description">{{ description }}</small></span>
    <span v-if="$slots.suffix" class="nh-menu-item__suffix"><slot name="suffix" /></span>
  </div>
</template>

<style scoped>
.nh-menu-item { display: flex; width: 100%; min-width: 0; min-height: 36px; align-items: center; padding: 7px 8px; border: 0; border-radius: 5px; color: var(--text-primary); background: transparent; font: inherit; text-align: left; cursor: pointer; gap: 8px; }
.nh-menu-item:hover:not(:disabled):not(.nh-menu-item--disabled) { background: var(--state-hover); }
.nh-menu-item:active:not(:disabled):not(.nh-menu-item--disabled) { background: var(--state-pressed); }
.nh-menu-item:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
.nh-menu-item--selected { color: var(--action-primary); background: color-mix(in srgb, var(--action-primary) 7%, transparent); }
.nh-menu-item--danger { color: var(--error); }
.nh-menu-item--disabled { cursor: default; opacity: .45; }
.nh-menu-item__prefix, .nh-menu-item__suffix { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; }
.nh-menu-item__content { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 1px; }
.nh-menu-item__content strong { overflow: hidden; font-size: 12px; font-weight: 500; line-height: 18px; text-overflow: ellipsis; white-space: nowrap; }
.nh-menu-item__content small { overflow: hidden; color: var(--text-secondary); font-size: 10px; line-height: 15px; text-overflow: ellipsis; white-space: nowrap; }
.nh-menu-item__suffix { margin-left: auto; color: var(--text-secondary); }
.nh-menu-item__divider { height: 1px; margin: 4px 8px; background: var(--border-divider); }
</style>
