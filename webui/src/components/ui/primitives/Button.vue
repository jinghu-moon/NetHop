<script setup lang="ts">
import { computed } from "vue";

type ButtonVariant = "default" | "primary" | "danger" | "outline" | "text";
type ButtonSize = "s" | "m" | "l";
type ButtonNativeType = "button" | "submit" | "reset";
type ButtonShape = "rounded" | "pill";

const props = withDefaults(defineProps<{
  variant?: ButtonVariant;
  size?: ButtonSize;
  shape?: ButtonShape;
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

const emit = defineEmits<{
  click: [event: MouseEvent];
}>();

const isDisabled = computed(() => props.disabled || props.loading);

function handleClick(event: MouseEvent): void {
  if (isDisabled.value) return;
  emit("click", event);
}
</script>

<template>
  <button
    class="nh-button"
    :class="[`nh-button--${variant}`, `nh-button--${size}`, `nh-button--shape-${shape}`, { 'nh-button--loading': loading }]"
    :type="nativeType"
    :disabled="isDisabled"
    :aria-busy="loading ? 'true' : undefined"
    @click="handleClick"
  >
    <span class="nh-button__content">
      <span class="nh-button__label"><slot /></span>
      <span v-if="loading" class="nh-button__spinner" aria-hidden="true"></span>
    </span>
  </button>
</template>

<style scoped>
.nh-button {
  --button-height: 36px;
  --button-padding-block: 7px;
  --button-padding-inline: 10px;
  --button-content-gap: 6px;
  --button-radius: 6px;
  --button-border: var(--border-default);
  --button-background: var(--surface);
  --button-color: var(--text-primary);
  --button-hover-background: var(--surface-component-hover);
  --button-active-background: var(--surface-muted);
  position: relative;
  display: inline-flex;
  min-width: 0;
  min-height: var(--button-height);
  align-items: center;
  justify-content: center;
  padding: var(--button-padding-block) var(--button-padding-inline);
  overflow: hidden;
  border: 1px solid var(--button-border);
  border-radius: var(--button-radius);
  color: var(--button-color);
  background: var(--button-background);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  line-height: 1;
  white-space: nowrap;
  cursor: pointer;
  touch-action: manipulation;
  transition: background-color .16s ease, border-color .16s ease, color .16s ease, opacity .16s ease, transform .12s ease;
  -webkit-tap-highlight-color: transparent;
}

.nh-button--s { --button-height: 32px; --button-padding-block: 5px; --button-padding-inline: 8px; --button-content-gap: 5px; font-size: 12px; }
.nh-button--l { --button-height: 44px; --button-padding-block: 9px; --button-padding-inline: 14px; --button-content-gap: 7px; font-size: 14px; }
.nh-button--shape-pill { --button-radius: 999px; }

.nh-button--primary {
  --button-border: var(--action-primary);
  --button-background: var(--action-primary);
  --button-color: var(--action-on-primary);
  --button-hover-background: var(--action-primary-hover);
  --button-active-background: var(--action-primary-active);
}

.nh-button--danger {
  --button-border: var(--error);
  --button-background: var(--error);
  --button-color: var(--text-inverse);
  --button-hover-background: var(--error-strong, var(--error));
  --button-active-background: var(--error-strong, var(--error));
}

.nh-button--outline,
.nh-button--text {
  --button-border: transparent;
  --button-background: transparent;
  --button-hover-background: var(--state-hover);
  --button-active-background: var(--state-pressed);
}

.nh-button--outline { --button-border: var(--border-default); }
.nh-button:hover:not(:disabled) { background: var(--button-hover-background); }
.nh-button:active:not(:disabled) {
  background: var(--button-active-background);
  border-color: var(--button-border);
  transform: none;
}
.nh-button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
.nh-button:disabled { cursor: default; opacity: .48; }
.nh-button__content { position: relative; display: inline-flex; min-height: 1em; min-width: 0; align-items: center; justify-content: center; gap: var(--button-content-gap); }
.nh-button__label { display: contents; }
.nh-button--loading .nh-button__label { visibility: hidden; }
.nh-button__spinner { position: absolute; width: 1em; height: 1em; border: 2px solid currentColor; border-right-color: transparent; border-radius: 50%; animation: nh-button-spin .7s linear infinite; }

@keyframes nh-button-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .nh-button { transition: none; }
  .nh-button__spinner { animation: none; }
}
</style>
