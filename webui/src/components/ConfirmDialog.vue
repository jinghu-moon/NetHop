<script setup lang="ts">
import { Dialog as TDialog, Button as TButton } from "tdesign-mobile-vue";
import { useBackDismiss } from "@/shell/useBackDispatcher";
const props = defineProps<{ visible: boolean; title: string; description: string; confirmLabel?: string }>();
const emit = defineEmits<{ "update:visible": [value: boolean]; confirm: [] }>();
useBackDismiss(() => props.visible, () => emit("update:visible", false));
</script>
<template><TDialog :visible="visible" :header="title" :close-on-overlay-click="false" @update:visible="emit('update:visible', $event)"><p>{{ description }}</p><template #footer><TButton variant="outline" @click="emit('update:visible', false)">取消</TButton><TButton theme="danger" @click="emit('confirm')">{{ confirmLabel ?? '确认' }}</TButton></template></TDialog></template>
