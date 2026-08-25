import type { ComputedRef, InjectionKey } from "vue";

export interface RadioGroupContext {
  name: ComputedRef<string | undefined>;
  disabled: ComputedRef<boolean>;
  modelValue: ComputedRef<string | undefined>;
  select(value: string, event?: Event): void;
}

export const radioGroupContextKey: InjectionKey<RadioGroupContext> = Symbol("nethop-radio-group");
