<script setup lang="ts">
import { computed, inject, useAttrs, useId } from "vue";
import { fieldContextKey } from "../form/field-context";

defineOptions({ inheritAttrs: false });

type CheckboxShape = "square" | "circle";

const props = withDefaults(defineProps<{
  modelValue?: boolean;
  shape?: CheckboxShape;
  disabled?: boolean;
  required?: boolean;
  invalid?: boolean;
  id?: string;
  name?: string;
  ariaLabel?: string;
}>(), {
  modelValue: false,
  shape: "square",
  disabled: false,
  required: false,
  invalid: false,
});

const attrs = useAttrs();
const generatedId = useId();
const field = inject(fieldContextKey, undefined);
const inputId = computed(() => props.id || field?.id.value || `nh-checkbox-${generatedId}`);
const disabled = computed(() => props.disabled || field?.disabled.value === true);
const required = computed(() => props.required || field?.required.value === true);
const invalid = computed(() => props.invalid || field?.invalid.value === true);
const describedBy = computed(() => typeof attrs["aria-describedby"] === "string" ? attrs["aria-describedby"] : field?.describedBy.value);
const label = computed(() => props.ariaLabel || (typeof attrs["aria-label"] === "string" ? attrs["aria-label"] : undefined));

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  change: [value: boolean, event: Event];
}>();

function handleChange(event: Event): void {
  const value = (event.target as HTMLInputElement).checked;
  emit("update:modelValue", value);
  emit("change", value, event);
}
</script>

<template>
  <label class="nh-checkbox" :class="[attrs.class, `nh-checkbox--${shape}`, { 'nh-checkbox--disabled': disabled, 'nh-checkbox--invalid': invalid }]">
    <input
      v-bind="attrs"
      class="nh-checkbox__control"
      type="checkbox"
      :id="inputId"
      :name="name"
      :checked="modelValue"
      :disabled="disabled"
      :required="required"
      :aria-label="label"
      :aria-invalid="invalid ? 'true' : undefined"
      :aria-required="required ? 'true' : undefined"
      :aria-describedby="describedBy"
      @change="handleChange"
    />
    <span class="nh-checkbox__box" aria-hidden="true"></span>
    <span v-if="$slots.default || $slots.label" class="nh-checkbox__label"><slot name="label"><slot /></slot></span>
  </label>
</template>

<style scoped>
.nh-checkbox { display: inline-flex; min-width: 0; align-items: center; gap: 7px; padding: 8px 6px; border-radius: 8px; color: var(--text-primary); font-size: 13px; line-height: 1.35; cursor: pointer; transition: background-color .13s cubic-bezier(.16,1,.3,1); }
.nh-checkbox:hover:not(.nh-checkbox--disabled) { background: var(--surface-component-hover, var(--surface-muted)); }
.nh-checkbox__control { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); clip-path: inset(50%); white-space: nowrap; }
.nh-checkbox__box { display: inline-grid; width: 18px; height: 18px; flex: 0 0 auto; place-items: center; border: 1.8px solid var(--border-default); border-radius: 5px; background: var(--surface); transition: background-color .14s cubic-bezier(.16,1,.3,1), border-color .13s ease, box-shadow .13s ease, transform .13s cubic-bezier(.34,1.56,.64,1); }
.nh-checkbox--circle .nh-checkbox__box { border-radius: 50%; }
.nh-checkbox__box::after { width: 8px; height: 5px; border-bottom: 2px solid var(--action-on-primary); border-left: 2px solid var(--action-on-primary); content: ""; opacity: 0; transform: rotate(-45deg) translate(1px, -1px) scale(.4); transition: opacity .12s ease, transform .2s cubic-bezier(.34,1.56,.64,1); }
.nh-checkbox__control:checked + .nh-checkbox__box { border-color: var(--action-primary); background: var(--action-primary); }
.nh-checkbox__control:checked + .nh-checkbox__box::after { opacity: 1; transform: rotate(-45deg) translate(1px, -1px) scale(1); }
.nh-checkbox:active:not(.nh-checkbox--disabled) .nh-checkbox__box { transform: scale(.9); }
.nh-checkbox__control:focus-visible + .nh-checkbox__box { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
.nh-checkbox--invalid .nh-checkbox__box { border-color: var(--error); }
.nh-checkbox--disabled { cursor: default; opacity: .5; }
@media (prefers-reduced-motion: reduce) { .nh-checkbox, .nh-checkbox__box, .nh-checkbox__box::after { transition: none; } }
</style>
