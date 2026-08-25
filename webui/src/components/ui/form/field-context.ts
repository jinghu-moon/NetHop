import type { ComputedRef, InjectionKey } from "vue";

export interface FieldContext {
  id: ComputedRef<string>;
  descriptionId: ComputedRef<string>;
  errorId: ComputedRef<string>;
  describedBy: ComputedRef<string | undefined>;
  invalid: ComputedRef<boolean>;
  required: ComputedRef<boolean>;
  disabled: ComputedRef<boolean>;
}

export const fieldContextKey: InjectionKey<FieldContext> = Symbol("nethop-field");
