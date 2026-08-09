import { computed, type Ref } from "vue";
import { useVirtualizer, type Virtualizer } from "@tanstack/vue-virtual";

export function useBoundedVirtualizer<T>(container: Ref<HTMLElement | undefined>, items: Ref<readonly T[]>, getItemKey: (index: number, item: T) => string, estimateSize = 56): Ref<Virtualizer<HTMLElement, Element>> {
  const options = computed(() => ({
    count: Math.min(items.value.length, 10_000),
    getScrollElement: () => container.value ?? null,
    estimateSize: () => estimateSize,
    getItemKey: (index: number) => { const item = items.value[index]; return item === undefined ? String(index) : getItemKey(index, item); },
    overscan: 4,
  }));
  return useVirtualizer(options);
}
