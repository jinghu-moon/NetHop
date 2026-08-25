import { nextTick, ref, toValue, type ComputedRef, type Ref } from "vue";
import type { DropdownPlacement } from "@/components/ui/overlay/Dropdown.vue";

type MaybeRef<T> = T | Ref<T> | ComputedRef<T>;
type DropdownSide = "top" | "bottom";
type DropdownAlign = "start" | "center" | "end";

interface PositionOptions {
  open: MaybeRef<boolean>;
  trigger: Ref<HTMLElement | undefined>;
  panel: MaybeRef<HTMLElement | undefined>;
  placement: MaybeRef<DropdownPlacement>;
  cursorPoint: Ref<{ x: number; y: number } | undefined>;
  matchTriggerWidth: MaybeRef<boolean>;
}

const EDGE = 10;
const GAP = 6;

export function useDropdownPosition(options: PositionOptions) {
  const style = ref<Record<string, string>>({});
  const arrowStyle = ref<Record<string, string>>({});
  const side = ref<DropdownSide>("bottom");
  const align = ref<DropdownAlign>("start");
  const positioned = ref(false);

  function preferred(): { side: DropdownSide; align: DropdownAlign } {
    const placement = toValue(options.placement);
    return {
      side: placement.startsWith("top") ? "top" : "bottom",
      align: placement.endsWith("end") ? "end" : placement.endsWith("center") ? "center" : "start",
    };
  }

  async function update(): Promise<void> {
    if (!toValue(options.open) || !options.trigger.value) return;
    await nextTick();
    const panel = toValue(options.panel);
    if (!panel) return;

    const anchor = options.trigger.value.getBoundingClientRect();
    const viewport = window.visualViewport;
    const viewportLeft = viewport?.offsetLeft ?? 0;
    const viewportTop = viewport?.offsetTop ?? 0;
    const viewportWidth = viewport?.width ?? window.innerWidth;
    const viewportHeight = viewport?.height ?? window.innerHeight;
    const viewportRight = viewportLeft + viewportWidth;
    const viewportBottom = viewportTop + viewportHeight;

    const scroll = panel.querySelector<HTMLElement>(".nh-dropdown-panel__scroll");
    const width = Math.max(panel.scrollWidth, scroll?.scrollWidth ?? 0, toValue(options.matchTriggerWidth) ? anchor.width : 0, 176);
    const naturalHeight = (scroll?.scrollHeight ?? panel.scrollHeight) + 2;
    const height = Math.min(naturalHeight, viewportHeight - EDGE * 2);

    const placement = toValue(options.placement);
    const desired = preferred();
    let currentSide = desired.side;
    let currentAlign = desired.align;
    let left: number;
    let top: number;

    if (placement === "cursor" && options.cursorPoint.value) {
      const point = options.cursorPoint.value;
      left = point.x + GAP;
      top = point.y + GAP;
      if (left + width > viewportRight - EDGE) left = point.x - width - GAP;
      if (top + height > viewportBottom - EDGE) top = point.y - height - GAP;
      currentSide = top < point.y ? "top" : "bottom";
      currentAlign = "start";
    } else {
      const bottomTop = anchor.bottom + GAP;
      const topTop = anchor.top - height - GAP;
      const fitsBottom = bottomTop + height <= viewportBottom - EDGE;
      const fitsTop = topTop >= viewportTop + EDGE;
      if (currentSide === "bottom" && !fitsBottom && fitsTop) currentSide = "top";
      else if (currentSide === "top" && !fitsTop && fitsBottom) currentSide = "bottom";
      else if (!fitsBottom && !fitsTop) currentSide = anchor.top - viewportTop > viewportBottom - anchor.bottom ? "top" : "bottom";
      top = currentSide === "top" ? Math.max(viewportTop + EDGE, topTop) : Math.min(bottomTop, viewportBottom - height - EDGE);
      left = currentAlign === "end" ? anchor.right - width : currentAlign === "center" ? anchor.left + (anchor.width - width) / 2 : anchor.left;
    }

    left = Math.max(viewportLeft + EDGE, Math.min(left, viewportRight - width - EDGE));
    top = Math.max(viewportTop + EDGE, Math.min(top, viewportBottom - height - EDGE));
    const originAnchorX = placement === "cursor" && options.cursorPoint.value
      ? options.cursorPoint.value.x
      : anchor.left + anchor.width / 2;
    const originX = Math.max(12, Math.min(width - 12, originAnchorX - left));
    side.value = currentSide;
    align.value = currentAlign;
    style.value = {
      top: `${top}px`,
      left: `${left}px`,
      width: toValue(options.matchTriggerWidth) ? `${anchor.width}px` : `${width}px`,
      maxHeight: `${height}px`,
      transformOrigin: `${originX}px ${currentSide === "top" ? `calc(100% + ${GAP}px)` : `-${GAP}px`}`,
    };
    arrowStyle.value = { left: `${Math.max(12, Math.min(width - 22, originAnchorX - left - 5))}px` };
    positioned.value = true;
  }

  function reset(): void {
    positioned.value = false;
    style.value = {};
    arrowStyle.value = {};
  }

  return { style, arrowStyle, side, align, positioned, update, reset };
}
