<script setup lang="ts">
import { computed, useAttrs } from "vue";

type SwitchSize = "s" | "m" | "l";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: boolean;
  size?: SwitchSize;
  disabled?: boolean;
  loading?: boolean;
  ariaLabel?: string;
  onText?: string;
  offText?: string;
}>(), {
  modelValue: false,
  size: "m",
  disabled: false,
  loading: false,
});

const attrs = useAttrs();
const isDisabled = computed(() => props.disabled || props.loading);
const label = computed(() => props.ariaLabel || (typeof attrs["aria-label"] === "string" ? attrs["aria-label"] : undefined));

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  change: [value: boolean];
}>();

function toggle(): void {
  if (isDisabled.value) return;
  const next = !props.modelValue;
  emit("update:modelValue", next);
  emit("change", next);
}
</script>

<template>
  <button
    v-bind="attrs"
    class="nh-switch"
    :class="[`nh-switch--${size}`, { 'nh-switch--on': modelValue, 'nh-switch--loading': loading, 'nh-switch--with-text': onText || offText, 'nh-switch--with-icon': $slots['on-icon'] || $slots['off-icon'] }]"
    type="button"
    role="switch"
    :aria-label="label"
    :aria-checked="modelValue ? 'true' : 'false'"
    :aria-busy="loading ? 'true' : undefined"
    :disabled="isDisabled"
    @click="toggle"
  >
    <span class="nh-switch__track" aria-hidden="true">
      <span v-if="onText" class="nh-switch__label nh-switch__label--on">{{ onText }}</span>
      <span v-if="offText" class="nh-switch__label nh-switch__label--off">{{ offText }}</span>
      <span class="nh-switch__thumb">
        <span v-if="$slots['off-icon']" class="nh-switch__icon nh-switch__icon--off"><slot name="off-icon" /></span>
        <span v-if="$slots['on-icon']" class="nh-switch__icon nh-switch__icon--on"><slot name="on-icon" /></span>
        <span v-if="loading" class="nh-switch__spinner" />
      </span>
    </span>
  </button>
</template>

<style scoped>
.nh-switch {
  --switch-width: 46px;
  --switch-height: 26px;
  --switch-thumb: 20px;
  --switch-inset: 3px;
  --switch-shift: calc(var(--switch-width) - var(--switch-thumb) - (var(--switch-inset) * 2));
  position: relative;
  display: inline-flex;
  box-sizing: border-box;
  width: var(--switch-width);
  height: var(--switch-height);
  flex: 0 0 auto;
  padding: 0;
  align-items: center;
  border: 1px solid var(--border-default);
  border-radius: 999px;
  color: var(--text-primary);
  background: transparent;
  cursor: pointer;
  transition: background-color .2s cubic-bezier(.16,1,.3,1), border-color .13s ease, box-shadow .13s ease;
  -webkit-tap-highlight-color: transparent;
}
.nh-switch--s { --switch-width: 36px; --switch-height: 21px; --switch-thumb: 15px; --switch-inset: 3px; }
.nh-switch--l { --switch-width: 56px; --switch-height: 31px; --switch-thumb: 25px; --switch-inset: 3px; }
.nh-switch__track { position: absolute; inset: 0; display: block; border-radius: inherit; background: var(--surface-component); transition: background-color .2s cubic-bezier(.16,1,.3,1); }
.nh-switch__thumb { position: absolute; top: 50%; left: var(--switch-inset); display: flex; width: var(--switch-thumb); height: var(--switch-thumb); align-items: center; justify-content: center; border-radius: 50%; background: var(--surface); box-shadow: 0 1px 3px rgb(0 0 0 / .25), 0 1px 1px rgb(0 0 0 / .12); transform: translate(0, -50%); transition: transform .34s cubic-bezier(.34,1.56,.64,1), width .13s cubic-bezier(.16,1,.3,1), background-color .18s ease; }
.nh-switch--on { border-color: var(--action-primary); }
.nh-switch--on .nh-switch__track { background: var(--action-primary); }
.nh-switch--on .nh-switch__thumb { transform: translate(var(--switch-shift), -50%); }
.nh-switch:active:not(:disabled) .nh-switch__thumb { width: calc(var(--switch-thumb) + 6px); }
.nh-switch--on:active:not(:disabled) .nh-switch__thumb { transform: translate(calc(var(--switch-shift) - 6px), -50%); }
.nh-switch:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
.nh-switch:disabled { cursor: default; opacity: .48; }
.nh-switch__label { position: absolute; top: 0; bottom: 0; display: flex; align-items: center; color: var(--action-on-primary); font-size: 12px; font-weight: 800; letter-spacing: .03em; opacity: 0; pointer-events: none; transition: opacity .13s ease; }
.nh-switch__label--on { left: 7px; }
.nh-switch__label--off { right: 6px; color: var(--text-secondary); opacity: 1; }
.nh-switch--on .nh-switch__label--on { opacity: 1; }
.nh-switch--on .nh-switch__label--off { opacity: 0; }
.nh-switch__icon { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; color: var(--text-secondary); opacity: 0; transform: scale(.4) rotate(-35deg); transition: opacity .13s ease, transform .2s cubic-bezier(.34,1.56,.64,1); }
.nh-switch__icon :deep(svg) { width: 80%; height: 80%; }
.nh-switch__icon--off { opacity: 1; transform: none; }
.nh-switch--on .nh-switch__icon--on { color: var(--action-primary); opacity: 1; transform: none; }
.nh-switch--on .nh-switch__icon--off { opacity: 0; transform: scale(.4) rotate(35deg); }
.nh-switch__spinner { width: 80%; height: 80%; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: nh-switch-spin .7s linear infinite; }
.nh-switch--loading .nh-switch__thumb > :not(.nh-switch__spinner) { opacity: 0 !important; }
@keyframes nh-switch-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .nh-switch, .nh-switch__track, .nh-switch__thumb, .nh-switch__icon, .nh-switch__label { transition: none; } .nh-switch__spinner { animation: none; } }
</style>
