<script setup lang="ts">
import { useAttrs } from "vue";

defineOptions({ inheritAttrs: false });

withDefaults(defineProps<{ modelValue?: boolean; disabled?: boolean }>(), { modelValue: false, disabled: false });
const attrs = useAttrs();
const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>

<template>
  <details v-bind="attrs" class="nh-disclosure" :open="modelValue" :data-disabled="disabled" @toggle="emit('update:modelValue', ($event.currentTarget as HTMLDetailsElement).open)">
    <summary :aria-disabled="disabled ? 'true' : undefined" @click="disabled ? $event.preventDefault() : undefined"><slot name="summary" /></summary>
    <div class="nh-disclosure__content"><slot /></div>
  </details>
</template>

<style scoped>
.nh-disclosure { min-width: 0; }
.nh-disclosure summary { cursor: pointer; list-style: none; }
.nh-disclosure summary::-webkit-details-marker { display: none; }
.nh-disclosure[data-disabled="true"] summary { cursor: default; opacity: .52; }
.nh-disclosure__content { min-width: 0; }
</style>
