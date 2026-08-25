<script setup lang="ts">
import { computed, inject, ref, useAttrs, watch } from "vue";
import { IconMinus, IconPlus } from "@tabler/icons-vue";
import { fieldContextKey } from "../form/field-context";
import IconButton from "./IconButton.vue";

type InputNumberSize = "s" | "m" | "l";
type InputNumberVariant = "plain" | "outline";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: number | undefined;
  min?: number;
  max?: number;
  step?: number;
  precision?: number;
  size?: InputNumberSize;
  variant?: InputNumberVariant;
  disabled?: boolean;
  readonly?: boolean;
  required?: boolean;
  invalid?: boolean;
  name?: string;
  id?: string;
  ariaLabel?: string;
}>(), {
  size: "m",
  variant: "plain",
  step: 1,
  disabled: false,
  readonly: false,
  required: false,
  invalid: false,
});

const attrs = useAttrs();
const field = inject(fieldContextKey, undefined);
const focused = ref(false);
const editingValue = ref("");
const lastValidValue = ref<number | undefined>(props.modelValue);
const inputId = computed(() => props.id || field?.id.value);
const describedBy = computed(() => typeof attrs["aria-describedby"] === "string" ? attrs["aria-describedby"] : field?.describedBy.value);
const invalid = computed(() => props.invalid || field?.invalid.value === true);
const disabled = computed(() => props.disabled || field?.disabled.value === true);
const required = computed(() => props.required || field?.required.value === true);
const numericValue = computed(() => {
  const value = Number(editingValue.value);
  return editingValue.value.trim() !== "" && Number.isFinite(value) ? value : undefined;
});
const canDecrement = computed(() => !disabled.value && !props.readonly && (props.min === undefined || numericValue.value === undefined || numericValue.value > props.min));
const canIncrement = computed(() => !disabled.value && !props.readonly && (props.max === undefined || numericValue.value === undefined || numericValue.value < props.max));
const ariaValueNow = computed(() => numericValue.value === undefined ? undefined : String(numericValue.value));

const emit = defineEmits<{
  "update:modelValue": [value: number | undefined];
  input: [value: number | undefined, event: Event];
  change: [value: number | undefined, event: Event];
  focus: [event: FocusEvent];
  blur: [event: FocusEvent];
}>();

function decimals(value: number): number {
  const text = String(value);
  const index = text.indexOf(".");
  return index < 0 ? 0 : text.length - index - 1;
}

function round(value: number): number {
  if (props.precision === undefined) return value;
  const factor = 10 ** Math.max(0, props.precision);
  return Math.round((value + Number.EPSILON) * factor) / factor;
}

function normalize(value: number): number {
  let next = round(value);
  if (props.min !== undefined) next = Math.max(props.min, next);
  if (props.max !== undefined) next = Math.min(props.max, next);
  const precision = props.precision ?? Math.max(decimals(props.step ?? 1), decimals(props.min ?? 0), decimals(props.max ?? 0));
  return Number(next.toFixed(precision));
}

function setValue(value: number | undefined, event?: Event): void {
  if (value === undefined) {
    editingValue.value = "";
    lastValidValue.value = undefined;
    emit("update:modelValue", undefined);
    if (event) emit("input", undefined, event);
    return;
  }
  const next = normalize(value);
  editingValue.value = String(next);
  lastValidValue.value = next;
  emit("update:modelValue", next);
  if (event) emit("input", next, event);
}

function commit(event: Event): void {
  const text = editingValue.value.trim();
  if (text === "") {
    setValue(undefined, event);
    emit("change", undefined, event);
    return;
  }
  const value = Number(text);
  if (!Number.isFinite(value)) {
    editingValue.value = lastValidValue.value === undefined ? "" : String(lastValidValue.value);
    return;
  }
  setValue(value, event);
  emit("change", lastValidValue.value, event);
}

function adjust(direction: -1 | 1): void {
  if (disabled.value || props.readonly) return;
  const current = numericValue.value ?? props.min ?? 0;
  setValue(current + direction * (props.step ?? 1));
}

function handleInput(event: Event): void {
  const value = (event.target as HTMLInputElement).value;
  editingValue.value = value;
  const parsed = Number(value);
  if (value.trim() !== "" && Number.isFinite(parsed)) setValue(parsed, event);
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === "ArrowUp") { event.preventDefault(); adjust(1); }
  else if (event.key === "ArrowDown") { event.preventDefault(); adjust(-1); }
  else if (event.key === "Enter") { commit(event); }
}

watch(() => props.modelValue, (value) => {
  if (focused.value) return;
  editingValue.value = value === undefined ? "" : String(normalize(value));
  lastValidValue.value = value;
}, { immediate: true });
</script>

<template>
  <span
    class="nh-input-number"
    :class="[`nh-input-number--${size}`, `nh-input-number--${variant}`, { 'nh-input-number--invalid': invalid, 'nh-input-number--disabled': disabled }]"
  >
    <IconButton
      class="nh-input-number__stepper"
      variant="text"
      :size="size"
      aria-label="减少"
      :disabled="!canDecrement"
      @click="adjust(-1)"
    ><IconMinus aria-hidden="true" /></IconButton>
    <input
      v-bind="attrs"
      class="nh-input-number__control"
      :id="inputId"
      :name="name"
      type="text"
      inputmode="decimal"
      role="spinbutton"
      :value="editingValue"
      :disabled="disabled"
      :readonly="readonly"
      :required="required"
      :aria-label="ariaLabel || (typeof attrs['aria-label'] === 'string' ? attrs['aria-label'] : undefined)"
      :aria-invalid="invalid ? 'true' : undefined"
      :aria-required="required ? 'true' : undefined"
      :aria-describedby="describedBy"
      :aria-valuemin="min === undefined ? undefined : String(min)"
      :aria-valuemax="max === undefined ? undefined : String(max)"
      :aria-valuenow="ariaValueNow"
      @input="handleInput"
      @change="commit"
      @keydown="handleKeydown"
      @focus="focused = true; emit('focus', $event)"
      @blur="focused = false; commit($event); emit('blur', $event)"
    />
    <IconButton
      class="nh-input-number__stepper"
      variant="text"
      :size="size"
      aria-label="增加"
      :disabled="!canIncrement"
      @click="adjust(1)"
    ><IconPlus aria-hidden="true" /></IconButton>
  </span>
</template>

<style scoped>
.nh-input-number {
  --number-height: 38px;
  --number-radius: 8px;
  --number-border: transparent;
  --number-background: transparent;
  display: inline-flex;
  min-width: 0;
  max-width: 100%;
  min-height: var(--number-height);
  align-items: center;
  box-sizing: border-box;
  overflow: hidden;
  border: 1px solid var(--number-border);
  border-radius: var(--number-radius);
  color: var(--text-primary);
  background: var(--number-background);
}
.nh-input-number--s { --number-height: 32px; font-size: 12px; }
.nh-input-number--l { --number-height: 44px; font-size: 14px; }
.nh-input-number--outline { --number-border: var(--border-default); --number-background: var(--surface); }
.nh-input-number:focus-within { border-color: var(--focus-ring); box-shadow: 0 0 0 3px color-mix(in srgb, var(--focus-ring) 18%, transparent); }
.nh-input-number--invalid { --number-border: var(--error); }
.nh-input-number--disabled { opacity: .52; }
.nh-input-number__control { min-width: 0; width: 5em; flex: 1 1 auto; padding: 0 4px; border: 0; outline: 0; color: inherit; background: transparent; font: inherit; line-height: 1.4; text-align: center; }
.nh-input-number__stepper { flex: 0 0 auto; border: 0 !important; border-radius: 0 !important; }
.nh-input-number__stepper :deep(svg) { width: 16px; height: 16px; }
@media (prefers-reduced-motion: reduce) { .nh-input-number { transition: none; } }
</style>
