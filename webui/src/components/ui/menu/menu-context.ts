import type { ComputedRef, InjectionKey, Ref } from "vue";

export type MenuSemantic = "menu" | "listbox";

export interface MenuContext {
  semantic: ComputedRef<MenuSemantic>;
  modelValue: ComputedRef<string | undefined>;
  disabled: ComputedRef<boolean>;
  activeValue: Ref<string | undefined>;
  select(value: string | undefined, event?: Event): void;
}

export const menuContextKey: InjectionKey<MenuContext> = Symbol("nethop-menu");
