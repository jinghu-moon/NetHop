<script setup lang="ts">
import { computed, provide, useId } from "vue";
import { fieldContextKey } from "./field-context";
import FieldLabel from "./FieldLabel.vue";
import FieldDescription from "./FieldDescription.vue";
import FieldError from "./FieldError.vue";

const props = withDefaults(defineProps<{
  label?: string;
  description?: string;
  error?: string;
  required?: boolean;
  disabled?: boolean;
  id?: string;
}>(), {
  label: "",
  description: "",
  error: "",
  required: false,
  disabled: false,
});

const generatedId = useId();
const controlId = computed(() => props.id || `nh-field-${generatedId}`);
const descriptionId = computed(() => props.description ? `${controlId.value}-description` : "");
const errorId = computed(() => props.error ? `${controlId.value}-error` : "");
const describedBy = computed(() => [descriptionId.value, errorId.value].filter(Boolean).join(" ") || undefined);
const invalid = computed(() => Boolean(props.error));
const required = computed(() => props.required);
const disabled = computed(() => props.disabled);

provide(fieldContextKey, { id: controlId, descriptionId, errorId, describedBy, invalid, required, disabled });
</script>

<template>
  <div class="nh-field" :class="{ 'nh-field--disabled': disabled, 'nh-field--invalid': invalid }">
    <FieldLabel v-if="label || $slots.label" :required="required"><slot name="label">{{ label }}</slot></FieldLabel>
    <div class="nh-field__control"><slot /></div>
    <FieldDescription v-if="description">{{ description }}</FieldDescription>
    <FieldError v-if="error">{{ error }}</FieldError>
  </div>
</template>

<style scoped>
.nh-field {
  display: grid;
  min-width: 0;
  gap: 7px;
  color: var(--text-primary);
}

.nh-field__control { min-width: 0; }
.nh-field--disabled { opacity: .58; }
</style>
