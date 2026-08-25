<script setup lang="ts">
withDefaults(defineProps<{ title: string; description?: string }>(), { description: "" });
</script>

<template>
  <section class="settings-group">
    <div class="settings-group-heading">
      <h3>{{ title }}</h3>
      <p v-if="description">{{ description }}</p>
    </div>
    <div class="settings-group-body"><slot /></div>
  </section>
</template>

<style scoped>
.settings-group { margin-bottom: 17px; }
.settings-group-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; padding: 0 4px 7px; }
.settings-group-heading h3 { margin: 0; color: var(--nh-muted); font-size: 11px; font-weight: 600; letter-spacing: .03em; text-transform: uppercase; }
.settings-group-heading p { margin: 0; overflow: hidden; color: var(--text-placeholder); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
.settings-group-body { overflow: hidden; border: 1px solid var(--nh-border); border-radius: 13px; background: var(--nh-surface); }
</style>
