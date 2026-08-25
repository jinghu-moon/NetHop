<script setup lang="ts">
import { computed, inject, ref, useAttrs } from "vue";
import { fieldContextKey } from "../form/field-context";

type InputType = "text" | "url" | "search" | "password" | "email" | "tel";
type InputSize = "s" | "m" | "l";
type InputVariant = "plain" | "outline";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: string;
  type?: InputType;
  size?: InputSize;
  variant?: InputVariant;
  placeholder?: string | undefined;
  maxlength?: number | undefined;
  disabled?: boolean;
  readonly?: boolean;
  required?: boolean;
  invalid?: boolean;
  autocomplete?: string | undefined;
  name?: string | undefined;
  id?: string | undefined;
}>(), {
  modelValue: "",
  type: "text",
  size: "m",
  variant: "plain",
  placeholder: "",
  disabled: false,
  readonly: false,
  required: false,
  invalid: false,
});

const attrs = useAttrs();
const composing = ref(false);
const field = inject(fieldContextKey, undefined);
const inputId = computed(() => props.id || field?.id.value);
const inputDescribedBy = computed(() => typeof attrs["aria-describedby"] === "string" ? attrs["aria-describedby"] : field?.describedBy.value);
const inputInvalid = computed(() => props.invalid || field?.invalid.value === true);
const inputAriaInvalid = computed<"true" | "false" | undefined>(() => {
  if (inputInvalid.value) return "true";
  const value = attrs["aria-invalid"];
  return value === "true" || value === "false" ? value : undefined;
});
const inputRequired = computed(() => props.required || field?.required.value === true);
const inputDisabled = computed(() => props.disabled || field?.disabled.value === true);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  input: [value: string, event: Event];
  change: [value: string, event: Event];
  focus: [event: FocusEvent];
  blur: [event: FocusEvent];
  compositionstart: [event: CompositionEvent];
  compositionend: [event: CompositionEvent];
}>();

function currentValue(event: Event): string {
  return (event.target as HTMLInputElement).value;
}

function handleInput(event: Event): void {
  const value = currentValue(event);
  if (composing.value) return;
  emit("update:modelValue", value);
  emit("input", value, event);
}

function handleChange(event: Event): void {
  emit("change", currentValue(event), event);
}

function handleCompositionStart(event: CompositionEvent): void {
  composing.value = true;
  emit("compositionstart", event);
}

function handleCompositionEnd(event: CompositionEvent): void {
  composing.value = false;
  const value = currentValue(event);
  emit("update:modelValue", value);
  emit("input", value, event);
  emit("compositionend", event);
}
</script>

<template>
  <span
    class="nh-input"
    :class="[`nh-input--${size}`, `nh-input--${variant}`, { 'nh-input--invalid': inputInvalid, 'nh-input--disabled': inputDisabled }]"
  >
    <span v-if="$slots.prefix" class="nh-input__prefix" aria-hidden="true"><slot name="prefix" /></span>
    <input
      v-bind="attrs"
      :id="inputId"
      :name="name"
      :type="type"
      :value="modelValue"
      :placeholder="placeholder"
      :maxlength="maxlength"
      :autocomplete="autocomplete"
      :disabled="inputDisabled"
      :readonly="readonly"
      :required="inputRequired"
      :aria-invalid="inputAriaInvalid"
      :aria-required="inputRequired ? 'true' : undefined"
      :aria-describedby="inputDescribedBy"
      @input="handleInput"
      @change="handleChange"
      @focus="emit('focus', $event)"
      @blur="emit('blur', $event)"
      @compositionstart="handleCompositionStart"
      @compositionend="handleCompositionEnd"
    />
    <span v-if="$slots.suffix" class="nh-input__suffix"><slot name="suffix" /></span>
  </span>
</template>

<style scoped>
.nh-input {
  --input-height: 38px;
  --input-padding-inline: 10px;
  --input-radius: 8px;
  --input-border: transparent;
  --input-background: transparent;
  --input-color: var(--text-primary);
  --input-placeholder: var(--text-secondary);
  --input-focus: var(--focus-ring);
  display: inline-flex;
  min-width: 0;
  max-width: 100%;
  min-height: var(--input-height);
  align-items: center;
  box-sizing: border-box;
  overflow: hidden;
  border: 1px solid var(--input-border);
  border-radius: var(--input-radius);
  color: var(--input-color);
  background: var(--input-background);
  vertical-align: middle;
  transition: border-color .16s ease, box-shadow .16s ease, background-color .16s ease;
}

.nh-input--s { --input-height: 32px; --input-padding-inline: 8px; font-size: 12px; }
.nh-input--l { --input-height: 44px; --input-padding-inline: 12px; font-size: 14px; }
.nh-input--outline { --input-border: var(--border-default); --input-background: var(--surface); }
.nh-input:focus-within { border-color: var(--input-focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--input-focus) 18%, transparent); }
.nh-input--plain:focus-within { background: var(--surface); }
.nh-input--invalid { --input-border: var(--error); --input-focus: var(--error); }
.nh-input--disabled { opacity: .52; cursor: default; }
.nh-input--disabled:focus-within { box-shadow: none; }

.nh-input > input {
  min-width: 0;
  width: 100%;
  height: calc(var(--input-height) - 2px);
  flex: 1 1 auto;
  padding: 0 var(--input-padding-inline);
  border: 0;
  outline: 0;
  color: inherit;
  background: transparent;
  font: inherit;
  line-height: 1.4;
}

.nh-input > input::placeholder { color: var(--input-placeholder); opacity: .72; }
.nh-input > input:disabled, .nh-input > input:read-only { cursor: default; }
.nh-input__prefix, .nh-input__suffix { display: inline-flex; flex: 0 0 auto; align-items: center; justify-content: center; }
.nh-input__prefix { margin-left: var(--input-padding-inline); }
.nh-input__suffix { margin-right: var(--input-padding-inline); }
.nh-input__prefix :deep(svg), .nh-input__suffix :deep(svg) { width: 16px; height: 16px; }
.nh-input--s .nh-input__prefix :deep(svg), .nh-input--s .nh-input__suffix :deep(svg) { width: 14px; height: 14px; }
.nh-input--l .nh-input__prefix :deep(svg), .nh-input--l .nh-input__suffix :deep(svg) { width: 18px; height: 18px; }
@media (prefers-reduced-motion: reduce) { .nh-input { transition: none; } }
</style>
