<script setup lang="ts">
import Switch from "@/components/ui/primitives/Switch.vue";
import Input from "@/components/ui/primitives/Input.vue";
import Textarea from "@/components/ui/primitives/Textarea.vue";
import InputNumber from "@/components/ui/primitives/InputNumber.vue";
import Field from "@/components/ui/form/Field.vue";
import OptionDropdown from "@/components/ui/composite/OptionDropdown.vue";

export interface SettingsFieldControlDefinition {
  readonly id?: string;
  readonly label: string;
  readonly valueType: "bool" | "int" | "string" | "enum" | "array";
  readonly value: boolean | number | string | readonly string[];
  readonly options?: readonly string[];
  readonly minimum?: number;
  readonly maximum?: number;
  readonly disabledReason?: string;
}

const props = defineProps<{ field: SettingsFieldControlDefinition }>();
const emit = defineEmits<{ change: [value: boolean | number | string | readonly string[]] }>();
</script>

<template>
  <div class="settings-field-control" :data-disabled="Boolean(field.disabledReason)">
    <div v-if="field.valueType === 'bool'" class="settings-field-inline"><span>{{ field.label }}</span><Switch :model-value="Boolean(field.value)" :disabled="Boolean(field.disabledReason)" :aria-label="field.label" @change="emit('change', $event)" /></div>
    <div v-else-if="field.valueType === 'int'" class="settings-field-inline"><span>{{ field.label }}</span><InputNumber :model-value="Number(field.value)" variant="outline" :min="field.minimum ?? 0" :max="field.maximum ?? Number.MAX_SAFE_INTEGER" :precision="0" :disabled="Boolean(field.disabledReason)" :aria-label="field.label" @update:model-value="(next) => next !== undefined && emit('change', next)" /></div>
    <Field v-else-if="field.valueType === 'string'" :label="field.label" :disabled="Boolean(field.disabledReason)"><Input :model-value="String(field.value)" variant="outline" type="text" :disabled="Boolean(field.disabledReason)" @update:model-value="(next) => emit('change', next)" /></Field>
    <Field v-else-if="field.valueType === 'enum'" :label="field.label" :disabled="Boolean(field.disabledReason)"><OptionDropdown class="settings-field-option" :model-value="String(field.value)" :options="(field.options ?? []).map((option) => ({ value: option, label: option }))" :disabled="Boolean(field.disabledReason)" :aria-label="field.label" @update:model-value="emit('change', $event)" /></Field>
    <Field v-else :label="field.label" :disabled="Boolean(field.disabledReason)"><Textarea :model-value="Array.isArray(field.value) ? field.value.join('\n') : ''" variant="outline" :disabled="Boolean(field.disabledReason)" placeholder="每行一项" :min-rows="3" :max-rows="8" @update:model-value="(next) => emit('change', next.split(/\r?\n/).map((item) => item.trim()).filter(Boolean))" /></Field>
    <small v-if="field.disabledReason" class="settings-field-reason">{{ field.disabledReason }}</small>
  </div>
</template>

<style scoped>
.settings-field-control { display: block; padding: 10px 12px; background: transparent; }
.settings-field-control[data-disabled="true"] { opacity: .58; }
.settings-field-inline { display: flex; min-height: 28px; align-items: center; justify-content: space-between; gap: 12px; }
.settings-field-inline > span, .settings-field-stack > span { color: var(--nh-text); font-size: 13px; font-weight: 550; }
.settings-field-option { width: 100%; }
.settings-field-reason { display: block; margin-top: 7px; color: var(--nh-warning); font-size: 10px; line-height: 1.35; }
</style>
