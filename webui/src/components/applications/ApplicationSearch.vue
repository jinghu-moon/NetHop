<script setup lang="ts">
import { IconSearch, IconX } from "@tabler/icons-vue";
import Input from "@/components/ui/primitives/Input.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";

defineProps<{ modelValue: string; placeholder?: string }>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <div class="application-search">
    <Input class="application-search__input" variant="outline" :model-value="modelValue" type="search" autocomplete="off" :placeholder="placeholder" spellcheck="false" @update:model-value="emit('update:modelValue', $event)">
      <template #prefix><IconSearch :size="18" /></template>
    </Input>
    <IconButton v-if="modelValue" class="application-search__clear" size="s" variant="text" aria-label="清除搜索" @click="emit('update:modelValue', '')"><IconX :size="15" /></IconButton>
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

.application-search__input { min-width: 0; flex: 1 1 auto; }
.application-search__input :deep(.nh-input) { width: 100%; min-height: 40px; border-color: transparent; background: transparent; }
.application-search__input :deep(.nh-input:focus-within) { border-color: transparent; box-shadow: none; }
.application-search__clear { flex: 0 0 auto; }
</style>
