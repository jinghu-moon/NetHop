export type OverlayType = "dialog" | "popup" | "action-sheet" | "dropdown" | "toast";
export type OverlayDismissReason = "escape" | "back" | "outside" | "backdrop" | "action" | "trigger";

export interface OverlayEntry {
  readonly id: number;
  readonly type: OverlayType;
  readonly modal: boolean;
  readonly dismissible: () => boolean;
  readonly close: (reason: OverlayDismissReason) => void;
  readonly closeOnOutside?: () => boolean;
  readonly contains?: (target: EventTarget | null) => boolean;
  open: boolean;
}

export interface OverlayToken {
  readonly id: number;
}

let nextId = 0;
const entries: OverlayEntry[] = [];
let listenerAttached = false;

function dispatchEscape(event: KeyboardEvent): void {
  if (event.key !== "Escape") return;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!entry?.open || !entry.dismissible()) continue;
    event.preventDefault();
    entry.close("escape");
    return;
  }
}

function dispatchOutside(event: PointerEvent): void {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!entry?.open || !entry.closeOnOutside?.() || entry.contains?.(event.target)) continue;
    entry.close("outside");
    return;
  }
}

function attachListener(): void {
  if (listenerAttached || typeof document === "undefined") return;
  document.addEventListener("keydown", dispatchEscape);
  document.addEventListener("pointerdown", dispatchOutside, true);
  listenerAttached = true;
}

function detachListener(): void {
  if (!listenerAttached || typeof document === "undefined") return;
  document.removeEventListener("keydown", dispatchEscape);
  document.removeEventListener("pointerdown", dispatchOutside, true);
  listenerAttached = false;
}

export function registerOverlay(entry: Omit<OverlayEntry, "id">): OverlayToken {
  const registered: OverlayEntry = { ...entry, id: ++nextId };
  entries.push(registered);
  attachListener();
  return { id: registered.id };
}

export function setOverlayOpen(token: OverlayToken, open: boolean): void {
  const entry = entries.find((candidate) => candidate.id === token.id);
  if (entry) entry.open = open;
}

export function unregisterOverlay(token: OverlayToken): void {
  const index = entries.findIndex((entry) => entry.id === token.id);
  if (index >= 0) entries.splice(index, 1);
  if (entries.length === 0) detachListener();
}

export function dispatchOverlayEscape(): boolean {
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    if (!entry?.open || !entry.dismissible()) continue;
    entry.close("escape");
    return true;
  }
  return false;
}

export function getOverlaySnapshot(): readonly OverlayEntry[] {
  return entries.map((entry) => ({ ...entry }));
}
