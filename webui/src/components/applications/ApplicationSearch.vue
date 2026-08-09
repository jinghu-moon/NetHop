<script setup lang="ts">
import { IconSearch, IconX } from "@tabler/icons-vue";

defineProps<{ modelValue: string; placeholder?: string }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

function update(event: Event): void {
  emit("update:modelValue", (event.target as HTMLInputElement).value);
}
</script>

<template>
  <div class="application-search">
    <IconSearch :size="18" />
    <input :value="modelValue" type="text" autocomplete="off" :placeholder="placeholder" spellcheck="false" @input="update" />
    <button v-if="modelValue" type="button" @click="emit('update:modelValue', '')"><IconX :size="15" /></button>
  </div>
</template>

<style scoped>
.application-search {
  display: flex;
  min-width: 0;
  min-height: 40px;
  align-items: center;
  padding: 0 11px;
  border: 1px solid var(--nh-border);
  border-radius: 6px;
  color: var(--nh-muted);
  background: var(--nh-surface);
  gap: 8px;
  transition: border-color .16s ease, background-color .16s ease;
}

.application-search:focus-within {
  border-color: var(--focus-ring);
}

.application-search > svg {
  flex: 0 0 auto;
}

.application-search input {
  width: 100%;
  min-width: 0;
  padding: 0;
  border: 0;
  outline: 0;
  color: var(--nh-text);
  background: transparent;
  font-size: 12px;
  line-height: 1.3;
}

.application-search input::placeholder {
  color: color-mix(in srgb, var(--nh-muted) 78%, transparent);
}

.application-search button {
  display: inline-flex;
  width: 24px;
  height: 24px;
  align-items: center;
  justify-content: center;
  padding: 0;
  border: 0;
  color: var(--nh-muted);
  background: transparent;
}
</style>
