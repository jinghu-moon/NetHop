<script setup lang="ts">
import { inject, useAttrs } from "vue";
import { fieldContextKey } from "./field-context";

defineOptions({ inheritAttrs: false });
const attrs = useAttrs();
const field = inject(fieldContextKey);
</script>

<template><p v-bind="attrs" :id="field?.descriptionId.value || (typeof attrs.id === 'string' ? attrs.id : undefined)" class="nh-field__description nh-field-description"><slot /></p></template>

<style scoped>
.nh-field-description { margin: 0; color: var(--text-secondary); font-size: 11px; line-height: 1.4; }
</style>
