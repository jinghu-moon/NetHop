<script setup lang="ts">
import SettingsPageHeader from "@/components/settings/SettingsPageHeader.vue";

withDefaults(defineProps<{
  visible: boolean;
  title: string;
  description?: string;
  loading?: boolean;
  canValidate?: boolean;
}>(), { description: "", loading: false, canValidate: false });

const emit = defineEmits<{
  back: [];
  reload: [];
  validate: [];
}>();
</script>

<template>
  <section
    class="settings-secondary-shell"
    :class="{ 'settings-secondary-shell--active': visible }"
    :aria-hidden="!visible"
    :inert="!visible"
  >
    <div v-if="visible" class="settings-secondary-content">
      <div class="settings-secondary-header">
        <SettingsPageHeader :title="title" :description="description" back :loading="loading" :can-validate="canValidate" @back="emit('back')" @reload="emit('reload')" @validate="emit('validate')" />
      </div>
      <div class="settings-secondary-body"><slot /></div>
    </div>
  </section>
</template>

<style scoped>
.settings-secondary-shell { position: absolute; z-index: 2; inset: 0; visibility: hidden; overflow: auto; padding: 0; border-radius: inherit; background: var(--nh-bg); pointer-events: none; transform: translateX(100%); transition: transform .35s cubic-bezier(.4, 0, .2, 1), visibility 0s linear .35s; will-change: transform; }
.settings-secondary-shell--active { visibility: visible; pointer-events: auto; transform: translateX(0); transition: transform .35s cubic-bezier(.4, 0, .2, 1), visibility 0s; }
.settings-secondary-content { min-width: 0; }
.settings-secondary-body { min-width: 0; }
@media (prefers-reduced-motion: reduce) {
  .settings-secondary-shell,
  .settings-secondary-shell--active { transition: none; }
}
</style>
