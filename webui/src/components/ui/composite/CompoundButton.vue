<script setup lang="ts">
import IconButton from "../primitives/IconButton.vue";
import Button from "../primitives/Button.vue";

type ButtonVariant = "default" | "primary" | "danger" | "outline" | "text";
type ButtonSize = "s" | "m" | "l";
type ButtonNativeType = "button" | "submit" | "reset";
type CompoundButtonOrientation = "horizontal" | "vertical";

const props = withDefaults(defineProps<{
  orientation?: CompoundButtonOrientation;
  size?: ButtonSize;
  iconVariant?: ButtonVariant;
  textVariant?: ButtonVariant;
  iconNativeType?: ButtonNativeType;
  textNativeType?: ButtonNativeType;
  iconLoading?: boolean;
  textLoading?: boolean;
  iconDisabled?: boolean;
  textDisabled?: boolean;
  iconAriaLabel: string;
}>(), {
  orientation: "horizontal",
  size: "m",
  iconVariant: "default",
  textVariant: "primary",
  iconNativeType: "button",
  textNativeType: "button",
  iconLoading: false,
  textLoading: false,
  iconDisabled: false,
  textDisabled: false,
});

const emit = defineEmits<{
  iconClick: [event: MouseEvent];
  textClick: [event: MouseEvent];
}>();
</script>

<template>
  <div class="nh-compound-button" :class="[`nh-compound-button--${props.orientation}`, `nh-compound-button--${props.size}`]">
    <IconButton
      class="nh-compound-button__icon"
      :variant="props.iconVariant"
      :size="props.size"
      :native-type="props.iconNativeType"
      :loading="props.iconLoading"
      :disabled="props.iconDisabled"
      :aria-label="props.iconAriaLabel"
      @click="emit('iconClick', $event)"
    >
      <slot name="icon" />
    </IconButton>
    <Button
      class="nh-compound-button__text"
      :variant="props.textVariant"
      :size="props.size"
      :native-type="props.textNativeType"
      :loading="props.textLoading"
      :disabled="props.textDisabled"
      @click="emit('textClick', $event)"
    >
      <slot />
    </Button>
  </div>
</template>

<style scoped>
.nh-compound-button { display: inline-flex; align-items: stretch; isolation: isolate; }
.nh-compound-button--vertical { flex-direction: column; }
.nh-compound-button--horizontal :deep(.nh-button) { border-radius: 0; }
.nh-compound-button--horizontal :deep(.nh-compound-button__icon) { border-top-left-radius: var(--button-radius, 6px); border-bottom-left-radius: var(--button-radius, 6px); }
.nh-compound-button--horizontal :deep(.nh-compound-button__text) { border-top-right-radius: var(--button-radius, 6px); border-bottom-right-radius: var(--button-radius, 6px); margin-left: -1px; }
.nh-compound-button--vertical :deep(.nh-button) { border-radius: 0; }
.nh-compound-button--vertical :deep(.nh-compound-button__icon) { border-top-left-radius: var(--button-radius, 6px); border-top-right-radius: var(--button-radius, 6px); }
.nh-compound-button--vertical :deep(.nh-compound-button__text) { border-bottom-right-radius: var(--button-radius, 6px); border-bottom-left-radius: var(--button-radius, 6px); margin-top: -1px; }
</style>
