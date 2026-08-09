<script setup lang="ts">
import { Button as TButton, Empty as TEmpty, Loading as TLoading, Result as TResult } from "tdesign-mobile-vue";
withDefaults(defineProps<{ kind: "empty" | "loading" | "error" | "stale"; title: string; detail?: string; actionLabel?: string }>(), { detail: "" });
const emit = defineEmits<{ action: [] }>();
</script>
<template>
  <div class="page-state" :data-kind="kind">
    <TLoading v-if="kind === 'loading'" layout="vertical" size="24px" :text="title" />
    <TEmpty v-else-if="kind === 'empty'" :description="detail || title"><template v-if="actionLabel" #action><TButton theme="primary" @click="emit('action')">{{ actionLabel }}</TButton></template></TEmpty>
    <template v-else><TResult :theme="kind === 'error' ? 'error' : 'warning'" :title="title" :description="detail" /><TButton v-if="actionLabel" theme="primary" @click="emit('action')">{{ actionLabel }}</TButton></template>
  </div>
</template>
