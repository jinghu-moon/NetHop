import { useStorage, type RemovableRef } from "@vueuse/core";

export type UiPreferenceKey = "theme" | "last-route" | "application-sort" | "application-selected-first" | "node-sort";
const allowedKeys: readonly UiPreferenceKey[] = ["theme", "last-route", "application-sort", "application-selected-first", "node-sort"];

export function uiStorageKey(key: UiPreferenceKey): `nethop.ui.${UiPreferenceKey}` {
  if (!allowedKeys.includes(key)) throw new Error("unsupported UI storage key");
  return `nethop.ui.${key}`;
}

export function useUiPreference(key: UiPreferenceKey, defaultValue: string): RemovableRef<string> {
  return useStorage(uiStorageKey(key), defaultValue, undefined, { writeDefaults: true });
}

export function isAllowedUiStorageKey(key: string): key is `nethop.ui.${UiPreferenceKey}` {
  return allowedKeys.some((value) => key === `nethop.ui.${value}`);
}
