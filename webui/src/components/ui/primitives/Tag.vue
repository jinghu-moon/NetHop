<script setup lang="ts">
type TagTone = "neutral" | "success" | "warning" | "danger" | "info";
type TagVariant = "soft" | "solid" | "outline";
type TagShape = "rounded" | "pill";
type TagSize = "s" | "m";

withDefaults(defineProps<{
  tone?: TagTone;
  variant?: TagVariant;
  shape?: TagShape;
  size?: TagSize;
}>(), {
  tone: "neutral",
  variant: "soft",
  shape: "rounded",
  size: "s",
});
</script>

<template>
  <span class="nh-tag" :class="[`nh-tag--${tone}`, `nh-tag--${variant}`, `nh-tag--${shape}`, `nh-tag--${size}`]">
    <span v-if="$slots.icon" class="nh-tag__icon" aria-hidden="true"><slot name="icon" /></span>
    <span class="nh-tag__label"><slot /></span>
    <span v-if="$slots.end" class="nh-tag__end"><slot name="end" /></span>
  </span>
</template>

<style scoped>
.nh-tag {
  --tag-border: var(--border-default);
  --tag-background: var(--surface-muted);
  --tag-color: var(--text-secondary);
  --tag-tone: var(--text-secondary);
  --tag-tone-strong: var(--text-secondary);
  --tag-container: var(--surface-muted);
  --tag-on-container: var(--text-secondary);
  display: inline-flex;
  min-width: 0;
  max-width: 100%;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border: 1px solid var(--tag-border);
  border-radius: 4px;
  color: var(--tag-color);
  background: var(--tag-background);
  font-weight: 600;
  line-height: 1;
  white-space: nowrap;
  text-overflow: ellipsis;
  vertical-align: middle;
}

.nh-tag--s { min-height: 20px; padding: 2px 6px; font-size: 10px; }
.nh-tag--m { min-height: 24px; padding: 3px 8px; font-size: 11px; }
.nh-tag--pill { border-radius: 999px; }

.nh-tag--soft {
  --tag-border: transparent;
  --tag-background: var(--tag-container);
  --tag-color: var(--tag-on-container);
}

.nh-tag--solid {
  --tag-border: var(--tag-tone);
  --tag-background: var(--tag-tone);
  --tag-color: var(--text-inverse);
}

.nh-tag--outline {
  --tag-border: var(--tag-tone);
  --tag-background: transparent;
  --tag-color: var(--tag-tone-strong);
}

.nh-tag__icon {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  margin-right: 4px;
}

.nh-tag__icon :deep(svg) { width: 12px; height: 12px; }
.nh-tag--m .nh-tag__icon :deep(svg) { width: 14px; height: 14px; }
.nh-tag__label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.nh-tag__end { display: inline-flex; flex: 0 0 auto; align-items: center; margin-left: 2px; }

.nh-tag--info {
  --tag-tone: var(--info);
  --tag-tone-strong: var(--info-strong);
  --tag-container: var(--info-container);
  --tag-on-container: var(--info-on-container);
}

.nh-tag--success {
  --tag-tone: var(--success);
  --tag-tone-strong: var(--success);
  --tag-container: var(--success-container);
  --tag-on-container: var(--success-on-container);
}

.nh-tag--warning {
  --tag-tone: var(--warning);
  --tag-tone-strong: var(--warning);
  --tag-container: var(--warning-container);
  --tag-on-container: var(--warning-on-container);
}

.nh-tag--danger {
  --tag-tone: var(--error);
  --tag-tone-strong: var(--error);
  --tag-container: var(--error-container);
  --tag-on-container: var(--error-on-container);
}
</style>
