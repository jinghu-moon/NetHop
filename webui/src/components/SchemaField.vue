<script setup lang="ts">
import { computed } from "vue";
import { Input as TInput, Switch as TSwitch, Textarea as TTextarea } from "tdesign-mobile-vue";
import OptionDropdown from "@/components/OptionDropdown.vue";
export interface SchemaFieldDefinition { readonly id: string; readonly label: string; readonly valueType: "bool" | "int" | "string" | "enum" | "array"; readonly value: boolean | number | string | readonly string[]; readonly options?: readonly string[]; readonly disabledReason?: string }
const props = defineProps<{ field: SchemaFieldDefinition }>();
const emit = defineEmits<{ change: [value: boolean | number | string | readonly string[]] }>();
const value = computed(() => props.field.value);
const enumOptions = computed(() => (props.field.options ?? []).map((item) => ({ value: item, label: item })));
</script>
<template><div class="schema-field"><TSwitch v-if="field.valueType === 'bool'" :value="Boolean(value)" :disabled="Boolean(field.disabledReason)" @change="emit('change', Boolean($event))">{{ field.label }}</TSwitch><TInput v-else-if="field.valueType === 'string' || field.valueType === 'int'" :label="field.label" :type="field.valueType === 'int' ? 'number' : 'text'" :value="String(value)" :disabled="Boolean(field.disabledReason)" @change="emit('change', field.valueType === 'int' ? Number($event) : String($event))" /><div v-else-if="field.valueType === 'enum'" class="schema-select"><span>{{ field.label }}</span><OptionDropdown :model-value="String(value)" :options="enumOptions" :disabled="Boolean(field.disabledReason)" @update:model-value="emit('change', $event)" /></div><TTextarea v-else :label="field.label" :value="Array.isArray(value) ? value.join('\n') : ''" :disabled="Boolean(field.disabledReason)" placeholder="每行一项" @change="emit('change', String($event).split(/\r?\n/).map((item) => item.trim()).filter(Boolean))" /><small v-if="field.disabledReason">{{ field.disabledReason }}</small></div></template>
