import { useDocumentVisibility, useEventListener, tryOnScopeDispose } from "@vueuse/core";
import { computed, watch, type Ref } from "vue";

import type { EventSession } from "./event-session";

export function useEventLifecycle(session: EventSession, visible?: Ref<boolean>): void {
  const documentVisibility = useDocumentVisibility();
  const effectiveVisibility = visible ?? computed(() => documentVisibility.value === "visible");
  session.start();
  const stop = watch(effectiveVisibility, (value) => session.setVisible(value), { immediate: true });
  useEventListener(window, "pagehide", () => session.stop());
  tryOnScopeDispose(() => { stop(); session.stop(); });
}
