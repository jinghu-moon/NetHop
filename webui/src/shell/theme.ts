import { onBeforeUnmount, onMounted, ref } from "vue";
import { useUiPreference } from "@/runtime/storage";

export type ThemeMode = "system" | "light" | "dark";

const mode = ref<ThemeMode>("system");
let initialized = false;

export function useTheme() {
  const preference = useUiPreference("theme", "system");
  if (!initialized) {
    mode.value = preference.value === "light" || preference.value === "dark" ? preference.value : "system";
    initialized = true;
  }
  const apply = (): void => {
    const dark = mode.value === "dark" || (mode.value === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    const resolved = dark ? "dark" : "light";
    document.documentElement.dataset.theme = resolved;
    document.documentElement.dataset.themeMode = resolved;
    document.documentElement.dataset.themePreference = mode.value;
    document.documentElement.setAttribute("theme-mode", resolved);
  };
  const setMode = (value: ThemeMode): void => { mode.value = value; preference.value = value; apply(); };
  const onSystem = (): void => { if (mode.value === "system") apply(); };
  onMounted(() => { apply(); window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", onSystem); });
  onBeforeUnmount(() => window.matchMedia("(prefers-color-scheme: dark)").removeEventListener("change", onSystem));
  return { mode, setMode, apply };
}
