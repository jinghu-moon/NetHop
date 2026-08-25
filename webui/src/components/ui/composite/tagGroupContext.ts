import type { ComputedRef } from "vue";

export type TagGroupValue = string;

export interface TagGroupContext {
  isSelected: (value: TagGroupValue) => boolean;
  toggle: (value: TagGroupValue) => void;
  disabled: ComputedRef<boolean>;
}

export const tagGroupContextKey = Symbol("nethop-tag-group");
