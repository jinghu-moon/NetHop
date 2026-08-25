<script setup lang="ts">
withDefaults(defineProps<{
  title: string;
  detail?: string;
  state?: "running" | "ready" | "degraded" | "error";
}>(), { detail: "", state: "ready" });
</script>

<template>
  <div class="settings-status-banner" :data-state="state">
    <span class="settings-status-icon" aria-hidden="true"><i></i></span>
    <div class="settings-status-copy">
      <strong>{{ title }}</strong>
      <span v-if="detail">{{ detail }}</span>
    </div>
    <span class="settings-status-dot" aria-hidden="true"></span>
  </div>
</template>

<style scoped>
.settings-status-banner { display: flex; min-width: 0; align-items: center; gap: 10px; margin-bottom: 18px; padding: 12px 13px; border: 1px solid var(--nh-border); border-radius: 13px; background: var(--nh-surface); }
.settings-status-icon { display: inline-flex; width: 32px; height: 32px; align-items: center; justify-content: center; flex: 0 0 32px; border-radius: 9px; background: color-mix(in srgb, var(--nh-info) 14%, transparent); }
.settings-status-icon i { width: 10px; height: 10px; border: 2px solid currentColor; border-radius: 50%; color: var(--nh-info); }
.settings-status-copy { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 2px; }
.settings-status-copy strong { overflow: hidden; font-size: 13px; line-height: 1.25; text-overflow: ellipsis; white-space: nowrap; }
.settings-status-copy span { overflow: hidden; color: var(--nh-muted); font-size: 10px; line-height: 1.3; text-overflow: ellipsis; white-space: nowrap; }
.settings-status-dot { width: 7px; height: 7px; flex: 0 0 7px; border-radius: 50%; background: var(--nh-success); }
.settings-status-banner[data-state="degraded"] .settings-status-icon { background: color-mix(in srgb, var(--nh-warning) 15%, transparent); }
.settings-status-banner[data-state="degraded"] .settings-status-icon i, .settings-status-banner[data-state="degraded"] .settings-status-dot { color: var(--nh-warning); background: var(--nh-warning); }
.settings-status-banner[data-state="error"] .settings-status-icon { background: color-mix(in srgb, var(--nh-danger) 15%, transparent); }
.settings-status-banner[data-state="error"] .settings-status-icon i, .settings-status-banner[data-state="error"] .settings-status-dot { color: var(--nh-danger); background: var(--nh-danger); }
</style>
