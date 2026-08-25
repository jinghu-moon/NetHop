<script setup lang="ts">
type DividerOrientation = "horizontal" | "vertical";
type DividerVariant = "solid" | "dashed";
type DividerAlign = "start" | "center" | "end";
type DividerInset = "none" | "s" | "m";

withDefaults(defineProps<{
  orientation?: DividerOrientation;
  variant?: DividerVariant;
  align?: DividerAlign;
  inset?: DividerInset;
  label?: string;
}>(), {
  orientation: "horizontal",
  variant: "solid",
  align: "center",
  inset: "none",
});
</script>

<template>
  <div
    class="nh-divider"
    :class="[
      `nh-divider--${orientation}`,
      `nh-divider--${variant}`,
      `nh-divider--align-${align}`,
      `nh-divider--inset-${inset}`,
      { 'nh-divider--with-label': Boolean(label) },
    ]"
    role="separator"
    :aria-orientation="orientation"
  >
    <span v-if="label && orientation === 'horizontal'" class="nh-divider__label">{{ label }}</span>
  </div>
</template>

<style scoped>
.nh-divider {
  --divider-color: var(--border-divider, var(--border-default));
  --divider-label-background: var(--surface, transparent);
  position: relative;
  display: flex;
  flex: 0 0 auto;
  color: var(--text-secondary);
}

.nh-divider--horizontal {
  width: auto;
  min-width: 0;
  min-height: 1px;
  align-items: center;
}

.nh-divider--horizontal::before,
.nh-divider--horizontal::after {
  display: block;
  flex: 1 1 auto;
  min-width: 0;
  height: 0;
  border-top: 1px solid var(--divider-color);
  content: "";
}

.nh-divider--horizontal:not(.nh-divider--with-label)::before { flex-basis: 100%; }
.nh-divider--horizontal:not(.nh-divider--with-label)::after { display: none; }
.nh-divider--dashed::before,
.nh-divider--dashed::after { border-top-style: dashed; }

.nh-divider--with-label .nh-divider__label {
  flex: 0 0 auto;
  padding: 0 12px;
  color: var(--text-secondary);
  background: var(--divider-label-background);
  font-size: 12px;
  line-height: 1.4;
  white-space: nowrap;
}

.nh-divider--with-label.nh-divider--align-start::before { flex-grow: 0; flex-basis: 24px; }
.nh-divider--with-label.nh-divider--align-start::after { flex-grow: 1; }
.nh-divider--with-label.nh-divider--align-end::before { flex-grow: 1; }
.nh-divider--with-label.nh-divider--align-end::after { flex-grow: 0; flex-basis: 24px; }
.nh-divider--with-label.nh-divider--align-center::before,
.nh-divider--with-label.nh-divider--align-center::after { flex-grow: 1; }

.nh-divider--vertical {
  width: 1px;
  min-width: 1px;
  min-height: 16px;
  height: auto;
  align-self: stretch;
  background: var(--divider-color);
}

.nh-divider--vertical.nh-divider--dashed {
  background: repeating-linear-gradient(to bottom, var(--divider-color) 0 4px, transparent 4px 8px);
}

.nh-divider--vertical .nh-divider__label { display: none; }

.nh-divider--inset-s.nh-divider--horizontal { margin-inline: 8px; }
.nh-divider--inset-m.nh-divider--horizontal { margin-inline: 16px; }
.nh-divider--inset-s.nh-divider--vertical { margin-block: 8px; }
.nh-divider--inset-m.nh-divider--vertical { margin-block: 16px; }

@media (prefers-reduced-motion: reduce) {
  .nh-divider { transition: none; }
}
</style>
