<script setup lang="ts">
import { inject, useAttrs } from "vue";
import { fieldContextKey } from "./field-context";

defineOptions({ inheritAttrs: false });
withDefaults(defineProps<{ required?: boolean }>(), { required: false });
const attrs = useAttrs();
const field = inject(fieldContextKey);
</script>

<template>
  <label v-bind="attrs" class="nh-field__label nh-field-label" :for="field?.id.value">
    <slot />
    <span v-if="required || field?.required.value" class="nh-field-label__required" aria-hidden="true">*</span>
  </label>
</template>

<style scoped>
.nh-field-label { display: inline-flex; align-items: baseline; color: var(--text-primary); font-size: 13px; font-weight: 600; line-height: 1.35; gap: 3px; }
.nh-field-label__required { color: var(--error); }
</style>
