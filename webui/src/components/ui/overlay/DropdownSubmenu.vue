<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { IconChevronRight } from "@tabler/icons-vue";
import Button from "../primitives/Button.vue";
import { useDropdownSubmenuPosition } from "@/composables/ui/use-dropdown-submenu-position";
import { subscribeFloatingLayer } from "@/infrastructure/overlay/floating-layer";
import { useDropdownSafeHover } from "@/composables/ui/use-dropdown-safe-hover";
import DropdownPanel from "./DropdownPanel.vue";
import { dropdownContextKey } from "./dropdown-context";

type SubmenuPhase = "closed" | "opening" | "open" | "closing";

const props = defineProps<{ label: string; disabled?: boolean }>();
const phase = ref<SubmenuPhase>("closed");
const pinned = ref(false);
const item = ref<HTMLElement>();
const trigger = computed(() => item.value?.querySelector<HTMLElement>("button") ?? undefined);
const panelComponent = ref<{ root?: HTMLElement }>();
const panel = computed(() => panelComponent.value?.root);
const position = useDropdownSubmenuPosition(trigger, panel);
const safeHover = useDropdownSafeHover(panel, position.side);
const context = inject(dropdownContextKey);
let unsubscribeFloating: (() => void) | undefined;
let unregisterSurface: (() => void) | undefined;
const expanded = computed(() => phase.value === "opening" || phase.value === "open");
const shouldRender = computed(() => phase.value !== "closed");

function startPositioning(): void {
  void nextTick(() => position.update());
  unsubscribeFloating ??= subscribeFloatingLayer(() => { void position.update(); });
}

function openSubmenu(): void {
  if (props.disabled || context?.open.value === false || expanded.value) return;
  safeHover.cancelClose();
  phase.value = "opening";
  startPositioning();
}

function finishClose(): void {
  phase.value = "closed";
  pinned.value = false;
  safeHover.cancelClose();
  position.reset();
  unsubscribeFloating?.();
  unsubscribeFloating = undefined;
}

function closeSubmenu(): void {
  pinned.value = false;
  safeHover.cancelClose();
  if (phase.value === "closed" || phase.value === "closing") return;
  if (!panel.value) { finishClose(); return; }
  phase.value = "closing";
}

function enter(): void {
  safeHover.cancelClose();
  openSubmenu();
}
function leave(event: MouseEvent): void { if (!pinned.value) safeHover.scheduleClose(event, closeSubmenu); }
function toggleClick(): void {
  if (expanded.value && pinned.value) {
    closeSubmenu();
    return;
  }
  pinned.value = true;
  openSubmenu();
}
function handleAnimationEnd(event: AnimationEvent): void {
  if (event.target !== panel.value) return;
  if (phase.value === "opening") phase.value = "open";
  else if (phase.value === "closing") finishClose();
}
watch(panel, (element) => {
  unregisterSurface?.();
  unregisterSurface = element ? context?.registerSurface(element) : undefined;
});
watch(() => context?.open.value, (parentOpen) => { if (parentOpen === false) closeSubmenu(); });
onBeforeUnmount(() => { safeHover.cancelClose(); unsubscribeFloating?.(); unregisterSurface?.(); });
</script>

<template>
  <li ref="item" class="nh-dropdown-submenu" :class="{ 'nh-dropdown-submenu--open': expanded, 'nh-dropdown-submenu--disabled': disabled }" @mouseenter="enter" @mouseleave="leave" @pointermove="safeHover.track">
    <Button class="nh-dropdown-submenu__trigger" variant="text" size="s" :disabled="disabled" :aria-expanded="expanded ? 'true' : 'false'" @pointerdown.stop @click.stop="toggleClick">
      <span><slot name="prefix" />{{ label }}</span>
      <IconChevronRight :size="15" aria-hidden="true" />
    </Button>
    <Teleport to="body"><DropdownPanel v-if="shouldRender && !disabled" ref="panelComponent" class="nh-dropdown-submenu__panel" :class="[`nh-dropdown-submenu__panel--${position.side.value}`, `nh-dropdown-submenu__panel--${phase}`]" :style="position.style.value" :data-positioned="position.positioned.value" :data-side="position.side.value" @animationend="handleAnimationEnd" @mouseenter="enter" @mouseleave="leave" @pointermove="safeHover.track"><slot /></DropdownPanel></Teleport>
  </li>
</template>

<style scoped>
.nh-dropdown-submenu { position: relative; min-width: 0; }
.nh-dropdown-submenu__trigger { display: flex; width: 100%; min-height: 36px; align-items: center; justify-content: space-between; padding: 8px 10px; border: 0; border-radius: 5px; color: var(--text-primary); background: transparent; font: inherit; font-size: 12px; text-align: left; gap: 10px; }
.nh-dropdown-submenu__trigger :deep(.nh-button__content) { width: 100%; flex: 1 1 auto; justify-content: space-between; gap: 10px; }
.nh-dropdown-submenu__trigger :deep(.nh-button__label > span) { display: inline-flex; min-width: 0; align-items: center; gap: 8px; }
.nh-dropdown-submenu__trigger { transition: background-color .14s ease, color .14s ease, box-shadow .14s ease; }
.nh-dropdown-submenu__trigger:hover:not(:disabled), .nh-dropdown-submenu--open .nh-dropdown-submenu__trigger { background: var(--state-hover); }
.nh-dropdown-submenu__trigger:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
.nh-dropdown-submenu__trigger:disabled { color: var(--text-disabled); cursor: default; opacity: .55; }
.nh-dropdown-submenu__panel { z-index: var(--overlay-z-dropdown-submenu, 910); max-width: min(280px, calc(100vw - 20px)); }
.nh-dropdown-submenu__panel--opening.nh-dropdown-submenu__panel--right { animation: nh-dropdown-submenu-in-right .18s cubic-bezier(.22,.61,.36,1) both; }
.nh-dropdown-submenu__panel--opening.nh-dropdown-submenu__panel--left { animation: nh-dropdown-submenu-in-left .18s cubic-bezier(.22,.61,.36,1) both; }
.nh-dropdown-submenu__panel--opening[data-positioned="false"] { animation-play-state: paused; }
.nh-dropdown-submenu__panel--closing { pointer-events: none; }
.nh-dropdown-submenu__panel--closing.nh-dropdown-submenu__panel--right { animation: nh-dropdown-submenu-out-right .14s cubic-bezier(.4,0,1,1) both; }
.nh-dropdown-submenu__panel--closing.nh-dropdown-submenu__panel--left { animation: nh-dropdown-submenu-out-left .14s cubic-bezier(.4,0,1,1) both; }
.nh-dropdown-submenu__panel[data-positioned="false"] { visibility: hidden; }
@keyframes nh-dropdown-submenu-in-right { from { opacity: 0; transform: translateX(-6px); } to { opacity: 1; transform: none; } }
@keyframes nh-dropdown-submenu-in-left { from { opacity: 0; transform: translateX(6px); } to { opacity: 1; transform: none; } }
@keyframes nh-dropdown-submenu-out-right { from { opacity: 1; transform: none; } to { opacity: 0; transform: translateX(-5px) scale(.97); } }
@keyframes nh-dropdown-submenu-out-left { from { opacity: 1; transform: none; } to { opacity: 0; transform: translateX(5px) scale(.97); } }
@media (prefers-reduced-motion: reduce) { .nh-dropdown-submenu__panel { animation-duration: .01ms; } }
</style>
