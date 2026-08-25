<script setup lang="ts">
import { IconArrowLeft, IconRefresh, IconShieldCheck } from "@tabler/icons-vue";
import Button from "@/components/ui/primitives/Button.vue";
import IconButton from "@/components/ui/primitives/IconButton.vue";

withDefaults(defineProps<{
  title: string;
  description?: string;
  back?: boolean;
  loading?: boolean;
  canValidate?: boolean;
}>(), { description: "", back: false, loading: false, canValidate: false });

const emit = defineEmits<{
  back: [];
  reload: [];
  validate: [];
}>();
</script>

<template>
  <header class="settings-header">
    <IconButton v-if="back" class="settings-icon-button" size="s" variant="outline" aria-label="返回设置" title="返回设置" @click="emit('back')">
      <IconArrowLeft :size="20" />
    </IconButton>
    <div class="settings-header-copy">
      <h2>{{ title }}</h2>
      <p v-if="description">{{ description }}</p>
    </div>
    <div class="settings-header-actions">
      <IconButton class="settings-icon-button" size="s" variant="outline" aria-label="重新加载" title="重新加载" :loading="loading" :disabled="loading" @click="emit('reload')">
        <IconRefresh :size="18" :class="{ 'settings-spin': loading }" />
      </IconButton>
      <Button class="settings-primary-button" size="s" variant="primary" :disabled="loading || !canValidate" @click="emit('validate')">
        <IconShieldCheck :size="16" />
        <span>验证</span>
      </Button>
    </div>
  </header>
</template>

<style scoped>
.settings-header { display: flex; min-width: 0; align-items: flex-start; gap: 10px; margin-bottom: 18px; }
.settings-header-copy { min-width: 0; flex: 1; padding-top: 1px; }
.settings-header-copy h2 { margin: 0; font-size: 23px; font-weight: 700; line-height: 1.22; }
.settings-header-copy p { margin: 4px 0 0; overflow: hidden; color: var(--nh-muted); font-size: 12px; line-height: 1.35; text-overflow: ellipsis; white-space: nowrap; }
.settings-header-actions { display: flex; align-items: center; gap: 6px; }
.settings-icon-button, .settings-primary-button { display: inline-flex; min-height: 34px; align-items: center; justify-content: center; border: 1px solid var(--nh-border); border-radius: 9px; color: var(--nh-text); background: var(--nh-surface); cursor: pointer; transition: transform .16s ease, background-color .16s ease, opacity .16s ease; }
.settings-icon-button { width: 34px; padding: 0; }
.settings-primary-button { padding: 0 11px; gap: 5px; color: var(--action-on-primary); border-color: var(--action-primary); background: var(--action-primary); font-size: 12px; font-weight: 600; }
.settings-icon-button:hover { background: var(--surface-component); }
.settings-icon-button:active, .settings-primary-button:active { transform: scale(.96); }
.settings-icon-button:disabled, .settings-primary-button:disabled { cursor: default; opacity: .48; }
.settings-spin { animation: settings-spin .8s linear infinite; }
@keyframes settings-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .settings-icon-button, .settings-primary-button { transition: none; } .settings-spin { animation: none; } }
</style>
