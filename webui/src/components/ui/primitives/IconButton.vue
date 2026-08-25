<script setup lang="ts">
import { useAttrs } from "vue";
import Button from "./Button.vue";

type ButtonVariant = "default" | "primary" | "danger" | "outline" | "text";
type ButtonSize = "s" | "m" | "l";
type ButtonNativeType = "button" | "submit" | "reset";
type IconButtonShape = "rounded" | "pill" | "circle";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  ariaLabel?: string;
  variant?: ButtonVariant;
  size?: ButtonSize;
  shape?: IconButtonShape;
  nativeType?: ButtonNativeType;
  loading?: boolean;
  disabled?: boolean;
}>(), {
  variant: "default",
  size: "m",
  shape: "rounded",
  nativeType: "button",
  loading: false,
  disabled: false,
});

const attrs = useAttrs();
</script>

<template>
  <Button
    v-bind="attrs"
    class="nh-icon-button"
    :class="{ 'nh-icon-button--circle': shape === 'circle' }"
    :variant="variant"
    :size="size"
    :shape="shape === 'circle' ? 'rounded' : shape"
    :native-type="nativeType"
    :loading="loading"
    :disabled="disabled"
    :aria-label="ariaLabel || attrs['aria-label']"
  >
    <slot />
  </Button>
</template>

<style scoped>
:global(.nh-button.nh-icon-button) {
  --icon-button-padding: 8px;
  box-sizing: border-box;
  width: var(--button-height);
  height: var(--button-height);
  min-width: var(--button-height);
  min-height: var(--button-height);
  aspect-ratio: 1;
  padding: var(--icon-button-padding);
}

:global(.nh-button.nh-icon-button.nh-button--s) {
  --icon-button-padding: 6px;
}

:global(.nh-button.nh-icon-button.nh-button--l) {
  --icon-button-padding: 10px;
}

:global(.nh-button.nh-icon-button.nh-icon-button--circle) {
  --button-radius: 50%;
}
</style>
