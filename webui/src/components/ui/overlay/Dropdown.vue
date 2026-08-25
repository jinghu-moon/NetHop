<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, useAttrs, watch } from "vue";
import { registerOverlay, setOverlayOpen, unregisterOverlay, type OverlayDismissReason, type OverlayToken } from "@/infrastructure/overlay/overlay-manager";
import { subscribeFloatingLayer } from "@/infrastructure/overlay/floating-layer";
import { useBackDismiss } from "@/shell/useBackDispatcher";
import { useDropdownPosition } from "@/composables/ui/use-dropdown-position";
import DropdownPanel from "./DropdownPanel.vue";
import { dropdownContextKey } from "./dropdown-context";

export type DropdownPlacement = "top-start" | "top-center" | "top-end" | "bottom-start" | "bottom-center" | "bottom-end" | "cursor";
export type DropdownPhase = "closed" | "opening" | "open" | "closing";
export type DropdownTrigger = "click" | "hover" | "context";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  open: boolean;
  trigger?: DropdownTrigger;
  placement?: DropdownPlacement;
  disabled?: boolean;
  showArrow?: boolean;
  closeOnOutside?: boolean;
  closeOnEscape?: boolean;
  closeOnSelect?: boolean;
  matchTriggerWidth?: boolean;
  panelClass?: string;
  panelWidth?: string;
  destroyOnClose?: boolean;
  openDelay?: number;
  closeDelay?: number;
}>(), {
  trigger: "click",
  placement: "bottom-start",
  disabled: false,
  showArrow: false,
  closeOnOutside: true,
  closeOnEscape: true,
  closeOnSelect: false,
  matchTriggerWidth: false,
  panelClass: "",
  panelWidth: "",
  destroyOnClose: true,
  openDelay: 70,
  closeDelay: 180,
});

defineSlots<{
  trigger(props: { open: boolean; toggle(): void; openMenu(): void; close(): void }): unknown;
  default(props: { open: boolean; close(): void; select(): void; activePanel: string; panelDepth: number; pushPanel(id: string): void; popPanel(): void }): unknown;
}>();

const attrs = useAttrs();
const emit = defineEmits<{
  "update:open": [value: boolean];
  opened: [];
  closed: [];
  dismiss: [reason: OverlayDismissReason];
}>();

const trigger = ref<HTMLElement>();
const panelComponent = ref<{ root?: HTMLElement; scroll?: HTMLElement }>();
const panel = computed(() => panelComponent.value?.root);
const panelScroll = computed(() => panelComponent.value?.scroll);
const shouldRender = ref(props.open);
const phase = ref<DropdownPhase>(props.open ? "opening" : "closed");
const panelStack = ref<string[]>(["root"]);
const direction = ref<"forward" | "backward">("forward");
const previousFocus = ref<HTMLElement>();
let token: OverlayToken | undefined;
let unsubscribeFloating: (() => void) | undefined;
let openTimer: number | undefined;
let closeTimer: number | undefined;
const cursorPoint = ref<{ x: number; y: number }>();
let panelObserver: ResizeObserver | undefined;
let positionFrame: number | undefined;

const activePanel = computed(() => panelStack.value.at(-1) ?? "root");
const placement = computed(() => props.placement);
const matchTriggerWidth = computed(() => props.matchTriggerWidth);
const rootOpen = computed(() => props.open);
const surfaces = new Set<HTMLElement>();
const position = useDropdownPosition({ open: computed(() => props.open), trigger, panel, placement, cursorPoint, matchTriggerWidth });

provide(dropdownContextKey, {
  open: rootOpen,
  registerSurface: (element) => {
    surfaces.add(element);
    return () => surfaces.delete(element);
  },
});

function clearTimers(): void {
  if (openTimer !== undefined) window.clearTimeout(openTimer);
  if (closeTimer !== undefined) window.clearTimeout(closeTimer);
  openTimer = undefined;
  closeTimer = undefined;
}

function openMenu(): void {
  if (props.disabled || props.open) return;
  previousFocus.value = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
  panelStack.value = ["root"];
  phase.value = "opening";
  shouldRender.value = true;
  emit("update:open", true);
  void nextTick(() => requestAnimationFrame(updatePosition));
}

function scheduleOpen(): void {
  clearTimers();
  if (props.open) return;
  openTimer = window.setTimeout(() => { openTimer = undefined; openMenu(); }, props.openDelay);
}

function scheduleClose(): void {
  clearTimers();
  if (!props.open) return;
  closeTimer = window.setTimeout(() => { closeTimer = undefined; requestClose("action"); }, props.closeDelay);
}

function handleTriggerClick(): void {
  if (props.trigger === "click") toggle();
}

function handleContextMenu(event: MouseEvent): void {
  if (props.trigger !== "context") return;
  event.preventDefault();
  cursorPoint.value = { x: event.clientX, y: event.clientY };
  if (props.open) requestClose("trigger");
  else openMenu();
}

function handleTriggerEnter(): void { if (props.trigger === "hover") scheduleOpen(); }
function handleTriggerLeave(): void { if (props.trigger === "hover") scheduleClose(); }
function handlePanelEnter(): void { if (props.trigger === "hover") clearTimers(); }
function handlePanelLeave(): void { if (props.trigger === "hover") scheduleClose(); }

function requestClose(reason: OverlayDismissReason = "action"): void {
  if (!props.open || phase.value === "closing") return;
  if (reason === "outside" && !props.closeOnOutside) return;
  if (reason === "escape" && !props.closeOnEscape) return;
  if ((reason === "back" || reason === "escape") && panelStack.value.length > 1) {
    popPanel();
    return;
  }
  emit("dismiss", reason);
  phase.value = "closing";
  emit("update:open", false);
}

function close(): void { requestClose("action"); }
function toggle(): void { if (props.open) close(); else openMenu(); }
function select(): void { if (props.closeOnSelect) close(); }

function pushPanel(id: string): void {
  if (!id || id === activePanel.value) return;
  direction.value = "forward";
  panelStack.value = [...panelStack.value, id];
}

function popPanel(): void {
  if (panelStack.value.length <= 1) return;
  direction.value = "backward";
  panelStack.value = panelStack.value.slice(0, -1);
}

function updatePosition(): void {
  if (positionFrame !== undefined) return;
  positionFrame = requestAnimationFrame(() => {
    positionFrame = undefined;
    void position.update();
  });
}

function prepareOpenPanel(): void {
  shouldRender.value = true;
  phase.value = "opening";
  void nextTick(() => {
    panelObserver?.disconnect();
    if (panelScroll.value) panelObserver?.observe(panelScroll.value);
    updatePosition();
  });
}

function handleAnimationEnd(event: AnimationEvent): void {
  if (event.target !== panel.value) return;
  if (phase.value === "opening") { phase.value = "open"; emit("opened"); }
  else if (phase.value === "closing") {
    phase.value = "closed";
    position.reset();
    if (props.destroyOnClose) shouldRender.value = false;
    previousFocus.value?.focus({ preventScroll: true });
    previousFocus.value = undefined;
    emit("closed");
  }
}

useBackDismiss(() => props.open, () => requestClose("back"));

onMounted(() => {
  token = registerOverlay({
    type: "dropdown",
    modal: false,
    dismissible: () => props.closeOnEscape,
    closeOnOutside: () => props.closeOnOutside,
    contains: (target) => target instanceof Node && Boolean(trigger.value?.contains(target) || panel.value?.contains(target) || Array.from(surfaces).some((surface) => surface.contains(target))),
    close: requestClose,
    open: props.open,
  });
  unsubscribeFloating = subscribeFloatingLayer(updatePosition);
  panelObserver = new ResizeObserver(updatePosition);
  if (props.open) prepareOpenPanel();
});

onBeforeUnmount(() => {
  clearTimers();
  if (positionFrame !== undefined) cancelAnimationFrame(positionFrame);
  panelObserver?.disconnect();
  if (token) unregisterOverlay(token);
  unsubscribeFloating?.();
});

watch(() => props.open, (open) => {
  if (token) setOverlayOpen(token, open);
  if (open) {
    prepareOpenPanel();
  } else if (shouldRender.value) phase.value = "closing";
});

watch(activePanel, () => { if (props.open) void nextTick(updatePosition); });
</script>

<template>
  <span ref="trigger" v-bind="attrs" class="nh-dropdown__trigger" :class="{ 'nh-dropdown__trigger--disabled': disabled }" :data-open="open" @click="handleTriggerClick" @contextmenu="handleContextMenu" @mouseenter="handleTriggerEnter" @mouseleave="handleTriggerLeave">
    <slot name="trigger" :open="open" :toggle="toggle" :open-menu="openMenu" :close="close" />
  </span>
  <Teleport to="body">
    <div v-if="shouldRender" class="nh-dropdown" :class="[`nh-dropdown--${phase}`, { 'nh-dropdown--inactive': phase === 'closed' }]" data-overlay-type="dropdown" :data-testid="typeof attrs['data-testid'] === 'string' ? attrs['data-testid'] : undefined">
      <DropdownPanel v-if="phase !== 'closed' || !destroyOnClose" ref="panelComponent" class="nh-dropdown__panel" :class="[panelClass, `nh-dropdown__panel--${position.side.value}`]" :style="{ ...position.style.value, width: panelWidth || position.style.value.width }" :data-positioned="position.positioned.value" :data-panel="activePanel" :data-side="position.side.value" :data-align="position.align.value" @animationend="handleAnimationEnd" @mouseenter="handlePanelEnter" @mouseleave="handlePanelLeave">
        <template #overlay><span v-if="showArrow" class="nh-dropdown__arrow" :style="position.arrowStyle.value" aria-hidden="true" /></template>
        <div class="nh-dropdown__content" :key="activePanel" :data-direction="direction">
          <slot :open="open" :close="close" :select="select" :active-panel="activePanel" :panel-depth="panelStack.length - 1" :push-panel="pushPanel" :pop-panel="popPanel" />
        </div>
      </DropdownPanel>
    </div>
  </Teleport>
</template>

<style scoped>
.nh-dropdown__trigger { display: inline-block; min-width: 0; }
.nh-dropdown__trigger--disabled { pointer-events: none; opacity: .5; }
.nh-dropdown { position: fixed; z-index: var(--overlay-z-dropdown, 900); inset: 0; pointer-events: none; }
.nh-dropdown--inactive { visibility: hidden; }
.nh-dropdown__panel[data-positioned="false"] { visibility: hidden; }
.nh-dropdown__content { min-width: 0; }
.nh-dropdown__arrow { position: absolute; z-index: -1; top: -5px; width: 10px; height: 10px; border-top: 1px solid var(--border-default); border-left: 1px solid var(--border-default); background: var(--surface); transform: rotate(45deg); }
.nh-dropdown__panel[data-side="top"] .nh-dropdown__arrow { top: auto; bottom: -5px; transform: rotate(225deg); }
.nh-dropdown--opening .nh-dropdown__panel[data-side="bottom"] { animation: nh-dropdown-in-bottom .22s cubic-bezier(.34,1.3,.64,1) both; }
.nh-dropdown--opening .nh-dropdown__panel[data-side="top"] { animation: nh-dropdown-in-top .22s cubic-bezier(.34,1.3,.64,1) both; }
.nh-dropdown--opening .nh-dropdown__panel[data-positioned="false"] { animation-play-state: paused; }
.nh-dropdown--closing .nh-dropdown__panel { pointer-events: none; }
.nh-dropdown--closing .nh-dropdown__panel[data-side="bottom"] { animation: nh-dropdown-out-bottom .14s cubic-bezier(.4,0,1,1) both; }
.nh-dropdown--closing .nh-dropdown__panel[data-side="top"] { animation: nh-dropdown-out-top .14s cubic-bezier(.4,0,1,1) both; }
.nh-dropdown__content[data-direction="forward"] { animation: nh-dropdown-panel-forward .18s cubic-bezier(.22,.61,.36,1) both; }
.nh-dropdown__content[data-direction="backward"] { animation: nh-dropdown-panel-backward .18s cubic-bezier(.22,.61,.36,1) both; }
@keyframes nh-dropdown-in-bottom { from { opacity: 0; transform: translateY(-6px) scale(.96, .9); } to { opacity: 1; transform: none; } }
@keyframes nh-dropdown-in-top { from { opacity: 0; transform: translateY(6px) scale(.96, .9); } to { opacity: 1; transform: none; } }
@keyframes nh-dropdown-out-bottom { from { opacity: 1; transform: none; } to { opacity: 0; transform: translateY(-4px) scale(.97, .92); } }
@keyframes nh-dropdown-out-top { from { opacity: 1; transform: none; } to { opacity: 0; transform: translateY(4px) scale(.97, .92); } }
@keyframes nh-dropdown-panel-forward { from { opacity: 0; transform: translateX(6px); } to { opacity: 1; transform: none; } }
@keyframes nh-dropdown-panel-backward { from { opacity: 0; transform: translateX(-6px); } to { opacity: 1; transform: none; } }
@media (prefers-reduced-motion: reduce) { .nh-dropdown--opening .nh-dropdown__panel, .nh-dropdown--closing .nh-dropdown__panel, .nh-dropdown__content { animation-duration: .01ms; } }
</style>
