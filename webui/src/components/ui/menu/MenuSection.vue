<script setup lang="ts">
import { useAttrs } from "vue";

defineOptions({ inheritAttrs: false });
defineProps<{ title?: string; divided?: boolean }>();
const attrs = useAttrs();
</script>

<template>
  <section v-bind="attrs" class="nh-menu-section" :class="{ 'nh-menu-section--divided': divided }">
    <h3 v-if="title || $slots.title" class="nh-menu-section__title"><slot name="title">{{ title }}</slot></h3>
    <div class="nh-menu-section__items"><slot /></div>
  </section>
</template>

<style scoped>
.nh-menu-section { min-width: 0; }
.nh-menu-section--divided { padding-top: 5px; border-top: 1px solid var(--border-divider); }
.nh-menu-section__title { margin: 2px 8px 4px; color: var(--text-secondary); font-size: 11px; font-weight: 600; line-height: 18px; }
.nh-menu-section__items { display: grid; gap: 2px; }
</style>
