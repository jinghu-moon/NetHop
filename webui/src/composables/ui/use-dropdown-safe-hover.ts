import { onBeforeUnmount, ref, type Ref } from "vue";

type Point = { x: number; y: number };

function isInsideTriangle(point: Point, a: Point, b: Point, c: Point): boolean {
  const cross = (p1: Point, p2: Point, p3: Point) => (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y);
  const d1 = cross(point, a, b);
  const d2 = cross(point, b, c);
  const d3 = cross(point, c, a);
  return !((d1 < 0 || d2 < 0 || d3 < 0) && (d1 > 0 || d2 > 0 || d3 > 0));
}

export function useDropdownSafeHover(panel: Ref<HTMLElement | undefined>, side: Ref<"left" | "right">, closeDelay = 120) {
  const pointer = ref({ x: 0, y: 0 });
  let closeTimer: number | undefined;
  let origin: Point | undefined;
  let deadline = 0;

  function cancelClose(): void {
    if (closeTimer !== undefined) window.clearTimeout(closeTimer);
    closeTimer = undefined;
    document.removeEventListener("pointermove", track);
  }

  function track(event: PointerEvent): void { pointer.value = { x: event.clientX, y: event.clientY }; }

  function shouldWait(): boolean {
    const rect = panel.value?.getBoundingClientRect();
    if (!rect || !origin) return false;
    const current = pointer.value;
    if (current.x >= rect.left && current.x <= rect.right && current.y >= rect.top && current.y <= rect.bottom) return true;
    const edgeX = side.value === "right" ? rect.left : rect.right;
    return performance.now() < deadline && isInsideTriangle(current, origin, { x: edgeX, y: rect.top - 8 }, { x: edgeX, y: rect.bottom + 8 });
  }

  function scheduleClose(event: MouseEvent, close: () => void): void {
    cancelClose();
    origin = { x: event.clientX, y: event.clientY };
    pointer.value = origin;
    deadline = performance.now() + 360;
    document.addEventListener("pointermove", track, { passive: true });
    const check = () => {
      if (shouldWait()) {
        closeTimer = window.setTimeout(check, 60);
        return;
      }
      cancelClose();
      close();
    };
    closeTimer = window.setTimeout(check, closeDelay);
  }

  onBeforeUnmount(cancelClose);
  return { track, cancelClose, scheduleClose };
}
