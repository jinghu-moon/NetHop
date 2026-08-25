import type { ComputedRef, InjectionKey } from "vue";

export interface DropdownContext {
  open: ComputedRef<boolean>;
  registerSurface(element: HTMLElement): () => void;
}

export const dropdownContextKey: InjectionKey<DropdownContext> = Symbol("nethop-dropdown");
