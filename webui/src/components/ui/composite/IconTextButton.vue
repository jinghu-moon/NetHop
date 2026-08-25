<script setup lang="ts">
import Button from "../primitives/Button.vue";

type ButtonVariant = "default" | "primary" | "danger" | "outline" | "text";
type ButtonSize = "s" | "m" | "l";
type ButtonNativeType = "button" | "submit" | "reset";
type IconTextButtonOrientation = "horizontal" | "vertical";
type IconTextButtonShape = "rounded" | "pill";

const props = withDefaults(defineProps<{
  orientation?: IconTextButtonOrientation;
  variant?: ButtonVariant;
  size?: ButtonSize;
  shape?: IconTextButtonShape;
  nativeType?: ButtonNativeType;
  loading?: boolean;
  disabled?: boolean;
}>(), {
  orientation: "horizontal",
  variant: "default",
  size: "m",
  shape: "rounded",
  nativeType: "button",
  loading: false,
  disabled: false,
});
</script>

<template>
  <Button
    class="nh-icon-text-button"
    :variant="props.variant"
    :size="props.size"
    :shape="props.shape"
    :native-type="props.nativeType"
    :loading="props.loading"
    :disabled="props.disabled"
  >
    <span class="nh-icon-text-button__content" :class="`nh-icon-text-button__content--${props.orientation}`">
      <span class="nh-icon-text-button__icon" aria-hidden="true"><slot name="icon" /></span>
      <span class="nh-icon-text-button__label"><slot /></span>
    </span>
  </Button>
</template>

<style scoped>
:global(.nh-button.nh-icon-text-button) {
  --icon-text-button-icon-size: 20px;
}

:global(.nh-button.nh-icon-text-button.nh-button--s) {
  --icon-text-button-icon-size: 18px;
}

:global(.nh-button.nh-icon-text-button.nh-button--l) {
  --icon-text-button-icon-size: 22px;
}

.nh-icon-text-button__content {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--button-content-gap, 6px);
}

.nh-icon-text-button__content--vertical {
  flex-direction: column;
}

.nh-icon-text-button__icon,
.nh-icon-text-button__label {
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.nh-icon-text-button__icon :deep(svg) {
  width: var(--icon-text-button-icon-size);
  height: var(--icon-text-button-icon-size);
  flex: 0 0 auto;
}
</style>
