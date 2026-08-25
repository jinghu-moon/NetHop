type UpdateCallback = () => void;

const callbacks = new Set<UpdateCallback>();
let attached = false;
let frame: number | undefined;

function schedule(): void {
  if (frame !== undefined) return;
  frame = requestAnimationFrame(() => {
    frame = undefined;
    callbacks.forEach((callback) => callback());
  });
}

function attach(): void {
  if (attached || typeof window === "undefined") return;
  window.addEventListener("resize", schedule);
  window.addEventListener("scroll", schedule, true);
  window.visualViewport?.addEventListener("resize", schedule);
  window.visualViewport?.addEventListener("scroll", schedule);
  attached = true;
}

function detach(): void {
  if (!attached || typeof window === "undefined") return;
  window.removeEventListener("resize", schedule);
  window.removeEventListener("scroll", schedule, true);
  window.visualViewport?.removeEventListener("resize", schedule);
  window.visualViewport?.removeEventListener("scroll", schedule);
  attached = false;
}

export function subscribeFloatingLayer(callback: UpdateCallback): () => void {
  callbacks.add(callback);
  attach();
  return () => {
    callbacks.delete(callback);
    if (callbacks.size === 0) detach();
  };
}
