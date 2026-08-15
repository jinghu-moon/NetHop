<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { IconWorld } from "@tabler/icons-vue";
import { territoryFlagAssets } from "@/generated/territory-assets";
import type { TerritoryCode } from "@/generated/territories";

const props = defineProps<{ readonly code?: TerritoryCode | undefined; readonly size?: "small" | "large" | undefined }>();
const failed = ref(false);
const source = computed(() => props.code === undefined ? undefined : territoryFlagAssets[props.code]);
watch(() => props.code, () => { failed.value = false; });
</script>

<template>
  <span class="territory-flag" :data-size="size ?? 'small'" :data-known="Boolean(source) && !failed">
    <img v-if="source && !failed" :src="source" @error="failed = true">
    <IconWorld v-else :size="size === 'large' ? 22 : 17" />
  </span>
</template>
