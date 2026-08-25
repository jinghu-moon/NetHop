<script setup lang="ts">
import { computed, inject, nextTick, provide, ref, useAttrs } from "vue";
import { fieldContextKey } from "./field-context";
import { radioGroupContextKey } from "./radio-group-context";

type RadioGroupValue = string;
type RadioGroupOrientation = "vertical" | "horizontal";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: RadioGroupValue;
  name?: string;
  disabled?: boolean;
  invalid?: boolean;
  orientation?: RadioGroupOrientation;
}>(), {
  disabled: false,
  invalid: false,
  orientation: "vertical",
});

const attrs = useAttrs();
const field = inject(fieldContextKey, undefined);
const root = ref<HTMLElement>();
const name = computed(() => props.name || (typeof attrs.name === "string" ? attrs.name : undefined));
const disabled = computed(() => props.disabled || field?.disabled.value === true);
const invalid = computed(() => props.invalid || field?.invalid.value === true);
const modelValue = computed(() => props.modelValue);

const emit = defineEmits<{
  "update:modelValue": [value: RadioGroupValue];
  change: [value: RadioGroupValue, event?: Event];
}>();

function select(value: RadioGroupValue, event?: Event): void {
  if (disabled.value || props.modelValue === value) return;
  emit("update:modelValue", value);
  emit("change", value, event);
}

provide(radioGroupContextKey, { name, disabled, modelValue, select });

function enabledRadios(): HTMLInputElement[] {
  return Array.from(root.value?.querySelectorAll<HTMLInputElement>('input[type="radio"]') ?? []).filter((radio) => !radio.disabled);
}

function handleKeydown(event: KeyboardEvent): void {
  if (!["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft", "Home", "End"].includes(event.key)) return;
  const radios = enabledRadios();
  if (radios.length === 0) return;

  const current = event.target instanceof HTMLInputElement ? radios.indexOf(event.target) : radios.findIndex((radio) => radio.checked);
  const currentIndex = current >= 0 ? current : 0;
  let nextIndex = currentIndex;
  if (event.key === "Home") nextIndex = 0;
  else if (event.key === "End") nextIndex = radios.length - 1;
  else if (event.key === "ArrowDown" || event.key === "ArrowRight") nextIndex = (currentIndex + 1) % radios.length;
  else if (event.key === "ArrowUp" || event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + radios.length) % radios.length;

  event.preventDefault();
  const next = radios[nextIndex];
  if (!next) return;
  next.focus();
  select(next.value, event);
  void nextTick(() => next.focus());
}
</script>

<template>
  <div
    ref="root"
    v-bind="attrs"
    class="nh-radio-group"
    :class="[`nh-radio-group--${orientation}`, { 'nh-radio-group--disabled': disabled, 'nh-radio-group--invalid': invalid }]"
    role="radiogroup"
    :aria-disabled="disabled ? 'true' : undefined"
    :aria-invalid="invalid ? 'true' : undefined"
    @keydown.capture="handleKeydown"
  >
    <slot />
  </div>
</template>

<style scoped>
.nh-radio-group { display: flex; min-width: 0; flex-direction: column; gap: 2px; }
.nh-radio-group--horizontal { flex-direction: row; flex-wrap: wrap; gap: 4px 8px; }
.nh-radio-group--disabled { opacity: .72; }
.nh-radio-group--invalid { --radio-group-invalid: var(--error); }
</style>
