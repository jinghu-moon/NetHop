<script setup lang="ts">
import { computed, nextTick, ref, useAttrs } from "vue";
import { IconEye, IconEyeOff } from "@tabler/icons-vue";
import Input from "../primitives/Input.vue";
import IconButton from "../primitives/IconButton.vue";

type InputSize = "s" | "m" | "l";
type InputVariant = "plain" | "outline";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  modelValue?: string;
  size?: InputSize;
  variant?: InputVariant;
  placeholder?: string | undefined;
  maxlength?: number;
  disabled?: boolean;
  readonly?: boolean;
  required?: boolean;
  invalid?: boolean;
  autocomplete?: string | undefined;
  name?: string | undefined;
  id?: string | undefined;
  showLabel?: string;
  hideLabel?: string;
}>(), {
  modelValue: "",
  size: "m",
  variant: "plain",
  placeholder: "",
  disabled: false,
  readonly: false,
  required: false,
  invalid: false,
  showLabel: "显示密码",
  hideLabel: "隐藏密码",
});

const attrs = useAttrs();
const root = ref<HTMLElement>();
const visible = ref(false);
const inputType = computed(() => visible.value ? "text" : "password");
const toggleLabel = computed(() => visible.value ? props.hideLabel : props.showLabel);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  input: [value: string, event: Event];
  change: [value: string, event: Event];
  focus: [event: FocusEvent];
  blur: [event: FocusEvent];
}>();

function toggleVisibility(): void {
  if (props.disabled) return;
  visible.value = !visible.value;
  void nextTick(() => {
    const input = root.value?.querySelector("input");
    if (input instanceof HTMLInputElement) {
      input.focus({ preventScroll: true });
      input.setSelectionRange(input.value.length, input.value.length);
    }
  });
}

function handleInput(value: string, event: Event): void {
  emit("input", value, event);
}

function handleChange(value: string, event: Event): void {
  emit("change", value, event);
}
</script>

<template>
  <span ref="root" class="nh-password-input">
    <Input
      v-bind="attrs"
      :model-value="modelValue"
      :type="inputType"
      :size="size"
      :variant="variant"
      :placeholder="placeholder"
      :maxlength="maxlength"
      :disabled="disabled"
      :readonly="readonly"
      :required="required"
      :invalid="invalid"
      :autocomplete="autocomplete"
      :name="name"
      :id="id"
      @update:model-value="emit('update:modelValue', $event)"
      @input="handleInput"
      @change="handleChange"
      @focus="emit('focus', $event)"
      @blur="emit('blur', $event)"
    >
      <template #suffix>
        <IconButton
          class="nh-password-input__toggle"
          size="s"
          variant="text"
          :aria-label="toggleLabel"
          :aria-pressed="visible ? 'true' : 'false'"
          :disabled="disabled"
          @mousedown.prevent
          @click="toggleVisibility"
        >
          <IconEyeOff v-if="visible" aria-hidden="true" />
          <IconEye v-else aria-hidden="true" />
        </IconButton>
      </template>
    </Input>
  </span>
</template>

<style scoped>
.nh-password-input { display: inline-block; min-width: 0; max-width: 100%; width: 100%; }
.nh-password-input :deep(.nh-input) { width: 100%; }
:global(.nh-button.nh-password-input__toggle) { width: 32px; min-width: 32px; height: 32px; min-height: 32px; padding: 6px; }
:global(.nh-button.nh-password-input__toggle) :deep(svg) { width: 16px; height: 16px; }
</style>
