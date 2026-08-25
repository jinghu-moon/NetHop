<script setup lang="ts">
import Button from "../primitives/Button.vue";

withDefaults(defineProps<{
  selected?: boolean;
  disabled?: boolean;
  danger?: boolean;
}>(), { selected: false, disabled: false, danger: false });

const emit = defineEmits<{ click: [event: MouseEvent] }>();
</script>

<template>
  <Button
    class="nh-menu-item-radio"
    :class="{ 'nh-menu-item-radio--selected': selected, 'nh-menu-item-radio--danger': danger }"
    variant="text"
    size="s"
    role="menuitemradio"
    :aria-checked="selected ? 'true' : 'false'"
    :data-selected="selected ? 'true' : 'false'"
    :disabled="disabled"
    @click="emit('click', $event)"
  >
    <span v-if="$slots.prefix" class="nh-menu-item-radio__prefix"><slot name="prefix" /></span>
    <span class="nh-menu-item-radio__content"><slot /></span>
    <span v-if="$slots.suffix" class="nh-menu-item-radio__suffix"><slot name="suffix" /></span>
  </Button>
</template>

<style scoped>
:global(.nh-button.nh-menu-item-radio) { display: flex; width: 100%; min-height: 35px; align-items: center; justify-content: flex-start; gap: 8px; padding: 6px 8px; border-radius: 5px; color: var(--text-primary); text-align: left; }
:global(.nh-button.nh-menu-item-radio.nh-menu-item-radio--selected) { color: var(--action-primary); background: color-mix(in srgb, var(--action-primary) 7%, transparent); }
:global(.nh-button.nh-menu-item-radio.nh-menu-item-radio--danger) { color: var(--error); }
.nh-menu-item-radio__prefix, .nh-menu-item-radio__suffix { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; }
.nh-menu-item-radio__content { min-width: 0; flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.nh-menu-item-radio__suffix { margin-left: auto; }
</style>
