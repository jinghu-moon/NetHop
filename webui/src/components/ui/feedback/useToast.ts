import { readonly, ref } from "vue";
import type { ToastItem } from "./toast-types";

const items = ref<ToastItem[]>([]);

function update(item: ToastItem): void {
  const index = items.value.findIndex((current) => current.id === item.id);
  if (index < 0) items.value = [...items.value, item];
  else items.value = items.value.map((current, currentIndex) => currentIndex === index ? item : current);
}

function dismiss(id: string): void { items.value = items.value.filter((item) => item.id !== id); }
function clear(): void { items.value = []; }

export function useToast() {
  return { items: readonly(items), update, dismiss, clear };
}
