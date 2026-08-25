<script setup lang="ts">
import { inject, useAttrs } from "vue";
import { fieldContextKey } from "./field-context";

defineOptions({ inheritAttrs: false });
const attrs = useAttrs();
const field = inject(fieldContextKey);
</script>

<template><p v-bind="attrs" :id="field?.errorId.value || (typeof attrs.id === 'string' ? attrs.id : undefined)" class="nh-field__error nh-field-error" role="alert"><slot /></p></template>

<style scoped>
.nh-field-error { margin: 0; color: var(--error); font-size: 11px; line-height: 1.4; }
</style>
