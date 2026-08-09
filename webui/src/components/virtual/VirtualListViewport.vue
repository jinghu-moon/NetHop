<script setup lang="ts" generic="T">
import { computed, ref } from "vue";
import { useBoundedVirtualizer } from "./useBoundedVirtualizer";
const props = withDefaults(defineProps<{ items: readonly T[]; getItemKey: (index: number, item: T) => string; estimateSize?: number }>(), { estimateSize: 56 });
const container = ref<HTMLElement>();
const items = computed(() => props.items);
const virtualizer = useBoundedVirtualizer(container, items, props.getItemKey, props.estimateSize);
const virtualRows = computed(() => container.value ? virtualizer.value.getVirtualItems() : []);
function itemAt(index: number): T { const item = items.value[index]; if (item === undefined) throw new Error("virtual item index is out of bounds"); return item; }
function scrollToStart(): void { container.value?.scrollTo({ top: 0, behavior: "smooth" }); }
function scrollToEnd(): void { container.value?.scrollTo({ top: container.value.scrollHeight, behavior: "smooth" }); }
defineExpose({ scrollToStart, scrollToEnd });
</script>
<template><div ref="container" class="virtual-viewport" style="height: 60dvh; overflow: auto; position: relative;"><div class="virtual-spacer" :style="{ height: `${virtualizer.getTotalSize()}px`, position: 'relative', width: '100%' }"><div v-for="row in virtualRows" :key="String(row.key)" class="virtual-row" :data-index="row.index" :ref="(element) => { if (element) virtualizer.measureElement(element as Element); }" :style="{ transform: `translateY(${row.start}px)`, position: 'absolute', top: '0', left: '0', width: '100%' }"><slot :item="itemAt(row.index)" :index="row.index" /></div></div></div></template>
