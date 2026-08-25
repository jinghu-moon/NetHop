import { nextTick, ref, type Ref } from "vue";

const EDGE = 10;
const GAP = 4;

export function useDropdownSubmenuPosition(trigger: Ref<HTMLElement | undefined>, panel: Ref<HTMLElement | undefined>) {
  const style = ref<Record<string, string>>({});
  const side = ref<"left" | "right">("right");
  const positioned = ref(false);

  async function update(): Promise<void> {
    await nextTick();
    if (!trigger.value || !panel.value) return;
    const anchor = trigger.value.getBoundingClientRect();
    const surface = panel.value.getBoundingClientRect();
    const viewport = window.visualViewport;
    const leftEdge = viewport?.offsetLeft ?? 0;
    const topEdge = viewport?.offsetTop ?? 0;
    const rightEdge = leftEdge + (viewport?.width ?? window.innerWidth);
    const bottomEdge = topEdge + (viewport?.height ?? window.innerHeight);
    const fitsRight = anchor.right + GAP + surface.width <= rightEdge - EDGE;
    side.value = fitsRight ? "right" : "left";
    const left = fitsRight ? anchor.right + GAP : Math.max(leftEdge + EDGE, anchor.left - surface.width - GAP);
    const top = Math.max(topEdge + EDGE, Math.min(anchor.top - 5, bottomEdge - surface.height - EDGE));
    style.value = { left: `${left}px`, top: `${top}px`, transformOrigin: `${fitsRight ? "left" : "right"} top` };
    positioned.value = true;
  }

  function reset(): void {
    style.value = {};
    positioned.value = false;
  }

  return { style, side, positioned, update, reset };
}
