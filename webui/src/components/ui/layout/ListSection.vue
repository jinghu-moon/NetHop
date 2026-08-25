<script setup lang="ts">
import { useAttrs } from "vue";

defineOptions({ inheritAttrs: false });

defineProps<{
  title?: string;
  description?: string;
}>();

const attrs = useAttrs();
</script>

<template>
  <section v-bind="attrs" class="nh-list-section">
    <header v-if="title || description || $slots.heading" class="nh-list-section__heading">
      <slot name="heading">
        <strong v-if="title">{{ title }}</strong>
        <small v-if="description">{{ description }}</small>
      </slot>
    </header>
    <slot />
  </section>
</template>

<style scoped>
.nh-list-section { min-width: 0; }
.nh-list-section__heading { display: flex; min-width: 0; flex-direction: column; gap: 3px; margin: 0 0 8px; }
.nh-list-section__heading strong { color: var(--text-primary); font-size: 13px; font-weight: 650; line-height: 1.35; }
.nh-list-section__heading small { color: var(--text-secondary); font-size: 11px; line-height: 1.4; }
</style>
