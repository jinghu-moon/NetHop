<script setup lang="ts">
withDefaults(defineProps<{
  title: string;
  description?: string;
  value?: string;
  clickable?: boolean;
  danger?: boolean;
  disabled?: boolean;
  arrow?: boolean;
}>(), { description: "", value: "", clickable: false, danger: false, disabled: false, arrow: false });

const emit = defineEmits<{ activate: [] }>();
</script>

<template>
  <component :is="clickable ? 'button' : 'div'" class="settings-row" :class="{ 'settings-row--clickable': clickable, 'settings-row--danger': danger, 'settings-row--disabled': disabled }" :disabled="clickable && disabled" type="button" @click="clickable && !disabled ? emit('activate') : undefined">
    <span v-if="$slots.icon" class="settings-row-icon"><slot name="icon" /></span>
    <span class="settings-row-copy">
      <strong>{{ title }}</strong>
      <small v-if="description">{{ description }}</small>
    </span>
    <span v-if="value" class="settings-row-value">{{ value }}</span>
    <span v-if="$slots.trailing" class="settings-row-trailing"><slot name="trailing" /></span>
    <svg v-if="arrow" class="settings-row-arrow" viewBox="0 0 24 24" aria-hidden="true"><path d="m9 6 6 6-6 6" /></svg>
  </component>
</template>

<style scoped>
.settings-row { position: relative; display: flex; width: 100%; min-height: 57px; align-items: center; gap: 11px; padding: 10px 13px; border: 0; color: var(--nh-text); background: transparent; text-align: left; }
.settings-row + .settings-row::before { position: absolute; top: 0; right: 0; left: 13px; border-top: 1px solid var(--nh-border); content: ""; }
.settings-row--clickable { cursor: pointer; transition: background-color .16s ease, transform .16s ease; }
.settings-row--clickable:active { background: var(--surface-component); transform: scale(.988); }
.settings-row--disabled { cursor: default; opacity: .5; }
.settings-row--danger .settings-row-copy strong { color: var(--nh-danger); }
.settings-row-icon { display: inline-flex; width: 27px; height: 27px; align-items: center; justify-content: center; flex: 0 0 27px; border-radius: 8px; color: var(--nh-muted); background: var(--nh-bg); }
.settings-row-copy { display: flex; min-width: 0; flex: 1; flex-direction: column; gap: 2px; }
.settings-row-copy strong { overflow: hidden; font-size: 13px; font-weight: 550; line-height: 1.25; text-overflow: ellipsis; white-space: nowrap; }
.settings-row-copy small { overflow: hidden; color: var(--nh-muted); font-size: 10px; line-height: 1.25; text-overflow: ellipsis; white-space: nowrap; }
.settings-row-value { max-width: 145px; overflow: hidden; color: var(--nh-muted); font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
.settings-row-trailing { display: inline-flex; align-items: center; flex: 0 0 auto; }
.settings-row-arrow { width: 15px; height: 15px; flex: 0 0 15px; fill: none; stroke: var(--text-placeholder); stroke-linecap: round; stroke-linejoin: round; stroke-width: 2.1; }
@media (prefers-reduced-motion: reduce) { .settings-row--clickable { transition: none; } }
</style>
