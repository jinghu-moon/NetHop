import { onActivated, onBeforeUnmount, onDeactivated, onMounted } from "vue";

export type BackHandler = () => boolean;

const handlers: BackHandler[] = [];

export function registerBackHandler(handler: BackHandler): () => void {
  handlers.push(handler);
  return () => {
    const index = handlers.lastIndexOf(handler);
    if (index >= 0) handlers.splice(index, 1);
  };
}

export function dispatchBack(): boolean {
  for (let index = handlers.length - 1; index >= 0; index -= 1) {
    if (handlers[index]?.()) return true;
  }
  return false;
}

export function useBackDispatcher() {
  const listener = (): void => { if (!dispatchBack()) window.history.back(); };
  window.addEventListener("nethop:back", listener);
  onBeforeUnmount(() => window.removeEventListener("nethop:back", listener));
  return { register: registerBackHandler, dispatch: dispatchBack };
}

export function useBackDismiss(isOpen: () => boolean, dismiss: () => void): void {
  let unregister: (() => void) | undefined;
  const activate = (): void => {
    unregister?.();
    unregister = registerBackHandler(() => {
      if (!isOpen()) return false;
      dismiss();
      return true;
    });
  };
  const deactivate = (): void => { unregister?.(); unregister = undefined; };
  onMounted(activate);
  onActivated(activate);
  onDeactivated(deactivate);
  onBeforeUnmount(deactivate);
}
