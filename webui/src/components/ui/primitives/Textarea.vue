<script setup lang="ts">
import { computed, inject, nextTick, onMounted, ref, useAttrs, watch } from "vue";
import { fieldContextKey } from "../form/field-context";

type TextareaSize = "s" | "m" | "l";
type TextareaVariant = "plain" | "outline";
type TextareaResize = "none" | "vertical";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: string;
  size?: TextareaSize;
  variant?: TextareaVariant;
  placeholder?: string | undefined;
  maxlength?: number;
  rows?: number;
  minRows?: number;
  maxRows?: number;
  resize?: TextareaResize;
  disabled?: boolean;
  readonly?: boolean;
  required?: boolean;
  invalid?: boolean;
  autocomplete?: string | undefined;
  name?: string | undefined;
  id?: string | undefined;
}>(), {
  modelValue: "",
  size: "m",
  variant: "plain",
  placeholder: "",
  rows: 3,
  resize: "vertical",
  disabled: false,
  readonly: false,
  required: false,
  invalid: false,
});

const attrs = useAttrs();
const composing = ref(false);
const textarea = ref<HTMLTextAreaElement>();
const field = inject(fieldContextKey, undefined);
const textareaId = computed(() => props.id || field?.id.value);
const describedBy = computed(() => typeof attrs["aria-describedby"] === "string" ? attrs["aria-describedby"] : field?.describedBy.value);
const invalid = computed(() => props.invalid || field?.invalid.value === true);
const ariaInvalid = computed<"true" | "false" | undefined>(() => {
  if (invalid.value) return "true";
  const value = attrs["aria-invalid"];
  return value === "true" || value === "false" ? value : undefined;
});
const required = computed(() => props.required || field?.required.value === true);
const disabled = computed(() => props.disabled || field?.disabled.value === true);
const effectiveMinRows = computed(() => Math.max(1, props.minRows ?? props.rows ?? 1));
const effectiveMaxRows = computed(() => {
  if (props.maxRows === undefined) return undefined;
  return Math.max(effectiveMinRows.value, props.maxRows);
});
const autosize = computed(() => props.minRows !== undefined || props.maxRows !== undefined);

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
  return (event.target as HTMLTextAreaElement).value;
}

function resizeTextarea(): void {
  const element = textarea.value;
  if (!element || !autosize.value) return;
  element.style.height = "auto";
  const styles = getComputedStyle(element);
  const lineHeight = Number.parseFloat(styles.lineHeight) || 19.6;
  const padding = (Number.parseFloat(styles.paddingTop) || 0) + (Number.parseFloat(styles.paddingBottom) || 0);
  const minHeight = lineHeight * effectiveMinRows.value + padding;
  const maxHeight = effectiveMaxRows.value === undefined ? Number.POSITIVE_INFINITY : lineHeight * effectiveMaxRows.value + padding;
  const height = Math.min(Math.max(element.scrollHeight, minHeight), maxHeight);
  element.style.height = `${height}px`;
  element.style.overflowY = element.scrollHeight > maxHeight ? "auto" : "hidden";
}

function scheduleResize(): void {
  if (autosize.value) void nextTick(resizeTextarea);
}

function handleInput(event: Event): void {
  const value = currentValue(event);
  if (composing.value) return;
  emit("update:modelValue", value);
  emit("input", value, event);
  scheduleResize();
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
  scheduleResize();
}

onMounted(scheduleResize);
watch(() => [props.modelValue, props.minRows, props.maxRows, props.size], scheduleResize);
</script>

<template>
  <span
    class="nh-textarea"
    :class="[`nh-textarea--${size}`, `nh-textarea--${variant}`, `nh-textarea--resize-${resize}`, { 'nh-textarea--invalid': invalid, 'nh-textarea--disabled': disabled }]"
  >
    <span v-if="$slots.prefix" class="nh-textarea__prefix" aria-hidden="true"><slot name="prefix" /></span>
    <textarea
      class="nh-textarea__control"
      ref="textarea"
      v-bind="attrs"
      :id="textareaId"
      :name="name"
      :value="modelValue"
      :placeholder="placeholder"
      :maxlength="maxlength"
      :rows="rows"
      :autocomplete="autocomplete"
      :disabled="disabled"
      :readonly="readonly"
      :required="required"
      :aria-invalid="ariaInvalid"
      :aria-required="required ? 'true' : undefined"
      :aria-describedby="describedBy"
      @input="handleInput"
      @change="handleChange"
      @focus="emit('focus', $event)"
      @blur="emit('blur', $event)"
      @compositionstart="handleCompositionStart"
      @compositionend="handleCompositionEnd"
    />
    <span v-if="$slots.suffix" class="nh-textarea__suffix"><slot name="suffix" /></span>
  </span>
</template>

<style scoped>
.nh-textarea {
  --textarea-padding-inline: 10px;
  --textarea-padding-block: 9px;
  --textarea-radius: 8px;
  --textarea-border: transparent;
  --textarea-background: transparent;
  --textarea-color: var(--text-primary);
  --textarea-placeholder: var(--text-secondary);
  --textarea-focus: var(--focus-ring);
  display: inline-flex;
  min-width: 0;
  max-width: 100%;
  align-items: stretch;
  box-sizing: border-box;
  overflow: hidden;
  border: 1px solid var(--textarea-border);
  border-radius: var(--textarea-radius);
  color: var(--textarea-color);
  background: var(--textarea-background);
  vertical-align: top;
  transition: border-color .16s ease, box-shadow .16s ease, background-color .16s ease;
}
.nh-textarea--s { --textarea-padding-inline: 8px; --textarea-padding-block: 7px; font-size: 12px; }
.nh-textarea--l { --textarea-padding-inline: 12px; --textarea-padding-block: 11px; font-size: 14px; }
.nh-textarea--outline { --textarea-border: var(--border-default); --textarea-background: var(--surface); }
.nh-textarea:focus-within { border-color: var(--textarea-focus); box-shadow: 0 0 0 3px color-mix(in srgb, var(--textarea-focus) 18%, transparent); }
.nh-textarea--plain:focus-within { background: var(--surface); }
.nh-textarea--invalid { --textarea-border: var(--error); --textarea-focus: var(--error); }
.nh-textarea--disabled { opacity: .52; cursor: default; }
.nh-textarea--disabled:focus-within { box-shadow: none; }
.nh-textarea > textarea {
  min-width: 0;
  width: 100%;
  flex: 1 1 auto;
  box-sizing: border-box;
  padding: var(--textarea-padding-block) var(--textarea-padding-inline);
  border: 0;
  outline: 0;
  color: inherit;
  background: transparent;
  font: inherit;
  line-height: 1.4;
}
.nh-textarea--resize-none > textarea { resize: none; }
.nh-textarea--resize-vertical > textarea { resize: vertical; }
.nh-textarea > textarea::placeholder { color: var(--textarea-placeholder); opacity: .72; }
.nh-textarea > textarea:disabled, .nh-textarea > textarea:read-only { cursor: default; }
.nh-textarea__prefix, .nh-textarea__suffix { display: inline-flex; flex: 0 0 auto; align-items: flex-start; justify-content: center; padding-top: var(--textarea-padding-block); }
.nh-textarea__prefix { margin-left: var(--textarea-padding-inline); }
.nh-textarea__suffix { margin-right: var(--textarea-padding-inline); }
.nh-textarea__prefix :deep(svg), .nh-textarea__suffix :deep(svg) { width: 16px; height: 16px; }
.nh-textarea--s .nh-textarea__prefix :deep(svg), .nh-textarea--s .nh-textarea__suffix :deep(svg) { width: 14px; height: 14px; }
.nh-textarea--l .nh-textarea__prefix :deep(svg), .nh-textarea--l .nh-textarea__suffix :deep(svg) { width: 18px; height: 18px; }
@media (prefers-reduced-motion: reduce) { .nh-textarea { transition: none; } }
</style>
