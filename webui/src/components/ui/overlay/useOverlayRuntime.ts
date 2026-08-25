import { nextTick, onBeforeUnmount, onMounted, type Ref, watch } from "vue";
import { useBackDismiss } from "@/shell/useBackDispatcher";
import { registerOverlay, setOverlayOpen, unregisterOverlay, type OverlayDismissReason, type OverlayToken, type OverlayType } from "@/infrastructure/overlay/overlay-manager";

export type { OverlayDismissReason } from "@/infrastructure/overlay/overlay-manager";

export interface OverlayRuntimeOptions {
  deferCloseCleanup?: boolean;
  type?: OverlayType;
  escapeDismissible?: () => boolean;
}

let scrollLockCount = 0;
let previousOverflow = "";
let modalLockCount = 0;
let modalRoot: HTMLElement | undefined;
let previousModalState: { inert: boolean; ariaHidden: string | null; pointerEvents: string } | undefined;

function lockScroll(): void {
  if (scrollLockCount === 0) {
    previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
  }
  scrollLockCount += 1;
}

function unlockScroll(): void {
  scrollLockCount = Math.max(0, scrollLockCount - 1);
  if (scrollLockCount === 0) document.body.style.overflow = previousOverflow;
}

function lockModalBackground(): void {
  const root = document.querySelector<HTMLElement>("#app");
  if (!root) return;
  if (modalLockCount === 0) {
    modalRoot = root;
    const inertRoot = root as HTMLElement & { inert?: boolean };
    previousModalState = {
      inert: inertRoot.inert === true,
      ariaHidden: root.getAttribute("aria-hidden"),
      pointerEvents: root.style.pointerEvents,
    };
    if ("inert" in inertRoot) inertRoot.inert = true;
    else {
      root.setAttribute("aria-hidden", "true");
      root.style.pointerEvents = "none";
    }
  }
  modalLockCount += 1;
}

function unlockModalBackground(): void {
  modalLockCount = Math.max(0, modalLockCount - 1);
  if (modalLockCount !== 0 || !modalRoot || !previousModalState) return;
  const inertRoot = modalRoot as HTMLElement & { inert?: boolean };
  inertRoot.inert = previousModalState.inert;
  if (previousModalState.ariaHidden === null) modalRoot.removeAttribute("aria-hidden");
  else modalRoot.setAttribute("aria-hidden", previousModalState.ariaHidden);
  modalRoot.style.pointerEvents = previousModalState.pointerEvents;
  modalRoot = undefined;
  previousModalState = undefined;
}

export function useOverlayRuntime(
  isOpen: () => boolean,
  dismiss: (reason?: OverlayDismissReason) => void,
  panel: Ref<HTMLElement | undefined>,
  options: OverlayRuntimeOptions = {},
): () => void {
  let locked = false;
  let modalLocked = false;
  let closeCleanupPending = false;
  let restoreFocus: HTMLElement | undefined;
  let token: OverlayToken | undefined;

  useBackDismiss(isOpen, () => dismiss("back"));

  function completeClose(): void {
    if (!closeCleanupPending && !locked) return;
    closeCleanupPending = false;
    if (locked) { unlockScroll(); locked = false; }
    if (modalLocked) { unlockModalBackground(); modalLocked = false; }
    restoreFocus?.focus({ preventScroll: true });
    restoreFocus = undefined;
  }

  watch(isOpen, (open) => {
    if (token) setOverlayOpen(token, open);
    if (open) {
      restoreFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
      if (!locked) { lockScroll(); locked = true; }
      if (!modalLocked) { lockModalBackground(); modalLocked = true; }
      closeCleanupPending = false;
      void nextTick(() => panel.value?.focus({ preventScroll: true }));
    } else {
      if (options.deferCloseCleanup) closeCleanupPending = true;
      else completeClose();
    }
  }, { immediate: true });

  onMounted(() => {
    token = registerOverlay({
      type: options.type ?? "popup",
      modal: true,
      dismissible: () => options.escapeDismissible?.() ?? true,
      close: (reason) => dismiss(reason),
      open: isOpen(),
    });
  });
  onBeforeUnmount(() => {
    if (token) unregisterOverlay(token);
    completeClose();
  });

  return completeClose;
}
