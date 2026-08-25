<script setup lang="ts">
import { computed, inject, useAttrs, useId } from "vue";
import { fieldContextKey } from "../form/field-context";
import { radioGroupContextKey } from "../form/radio-group-context";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: boolean;
  value?: string;
  name?: string;
  disabled?: boolean;
  id?: string;
  ariaLabel?: string;
}>(), { modelValue: false, disabled: false });

const attrs = useAttrs();
const generatedId = useId();
const field = inject(fieldContextKey, undefined);
const group = inject(radioGroupContextKey, undefined);
const radioId = computed(() => props.id || field?.id.value || `nh-radio-${generatedId}`);
const disabled = computed(() => props.disabled || field?.disabled.value === true || group?.disabled.value === true);
const radioName = computed(() => props.name || group?.name.value);
const checked = computed(() => group ? group.modelValue.value === props.value : props.modelValue);
const label = computed(() => props.ariaLabel || (typeof attrs["aria-label"] === "string" ? attrs["aria-label"] : undefined));

const emit = defineEmits<{ "update:modelValue": [value: boolean]; change: [value: boolean, event: Event] }>();
function handleChange(event: Event): void {
  const checked = (event.target as HTMLInputElement).checked;
  if (group && checked) group.select(props.value || "", event);
  emit("update:modelValue", checked);
  emit("change", checked, event);
}
</script>

<template>
  <label class="nh-radio" :class="[attrs.class, { 'nh-radio--disabled': disabled }]">
    <input v-bind="attrs" class="nh-radio__control" type="radio" :id="radioId" :name="radioName" :value="value" :checked="checked" :disabled="disabled" :aria-label="label" @change="handleChange" />
    <span class="nh-radio__dot" aria-hidden="true"></span>
    <span v-if="$slots.default || $slots.label" class="nh-radio__label"><slot name="label"><slot /></slot></span>
  </label>
</template>

<style scoped>
.nh-radio { display: inline-flex; min-width: 0; align-items: center; gap: 10px; padding: 8px 6px; border-radius: 8px; color: var(--text-primary); font-size: 13px; line-height: 1.35; cursor: pointer; transition: background-color .13s cubic-bezier(.16,1,.3,1); }
.nh-radio:hover:not(.nh-radio--disabled) { background: var(--surface-component-hover, var(--surface-muted)); }
.nh-radio__control { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); clip-path: inset(50%); }
.nh-radio__dot { display: inline-grid; width: 18px; height: 18px; flex: 0 0 auto; place-items: center; border: 1.8px solid var(--border-default); border-radius: 50%; background: var(--surface); transition: border-color .13s cubic-bezier(.16,1,.3,1), transform .13s cubic-bezier(.34,1.56,.64,1), box-shadow .13s ease; }
.nh-radio__dot::after { width: 9px; height: 9px; border-radius: 50%; background: var(--action-primary); content: ""; opacity: 0; transform: scale(0); transition: transform .2s cubic-bezier(.34,1.56,.64,1), opacity .12s ease; }
.nh-radio__control:checked + .nh-radio__dot { border-color: var(--action-primary); }
.nh-radio__control:checked + .nh-radio__dot::after { opacity: 1; transform: scale(1); }
.nh-radio__control:focus-visible + .nh-radio__dot { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
.nh-radio--disabled { cursor: default; opacity: .5; }
.nh-radio:active:not(.nh-radio--disabled) .nh-radio__dot { transform: scale(.9); }
@media (prefers-reduced-motion: reduce) { .nh-radio, .nh-radio__dot, .nh-radio__dot::after { transition: none; } }
</style>
