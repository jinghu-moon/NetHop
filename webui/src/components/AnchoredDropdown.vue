<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from "vue";
import { onClickOutside } from "@vueuse/core";
import { useBackDismiss } from "@/shell/useBackDispatcher";

interface DropdownSlotProps {
  readonly open: boolean;
  readonly toggle: () => void;
  readonly close: () => void;
}

interface DropdownMenuSlotProps {
  readonly activePanel: string;
  readonly panelDepth: number;
  readonly pushPanel: (panel: string) => void;
  readonly popPanel: () => void;
  readonly close: () => void;
}

const props = withDefaults(defineProps<{
  menuLabel: string;
  menuClass?: string;
  menuWidth?: string;
  align?: "start" | "end";
  offset?: number;
}>(), {
  menuClass: "",
  menuWidth: "auto",
  align: "end",
  offset: 6,
});
defineSlots<{
  trigger(props: DropdownSlotProps): unknown;
  default(props: DropdownMenuSlotProps): unknown;
}>();

const root = ref<HTMLElement>();
const panelStackElement = ref<HTMLElement>();
const panelElement = ref<HTMLElement>();
const open = ref(false);
const panelStack = ref<string[]>(["root"]);
const panelDirection = ref<"forward" | "backward">("forward");
const animatePanel = ref(false);
const panelHeight = ref<number>();
const resizingPanel = ref(false);
let resizeFrame: number | undefined;
let resizeTimer: number | undefined;
let resizeGeneration = 0;
const resizeFallbackMs = 380;
const activePanel = computed(() => panelStack.value.at(-1) ?? "root");
const menuStyle = computed(() => ({ width: props.menuWidth, top: `calc(100% + ${props.offset}px)` }));
const panelStackStyle = computed(() => panelHeight.value === undefined ? undefined : { height: `${panelHeight.value}px` });

function cancelPanelResize(): void {
  resizeGeneration += 1;
  if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
  if (resizeTimer !== undefined) window.clearTimeout(resizeTimer);
  resizeFrame = undefined;
  resizeTimer = undefined;
  resizingPanel.value = false;
  panelHeight.value = undefined;
}

function resetPanels(): void {
  cancelPanelResize();
  panelDirection.value = "forward";
  animatePanel.value = false;
  panelStack.value = ["root"];
}

function close(): void {
  open.value = false;
  resetPanels();
}

function toggle(): void {
  if (open.value) close();
  else {
    resetPanels();
    open.value = true;
  }
}

function transitionPanel(nextStack: string[], direction: "forward" | "backward"): void {
  const startHeight = panelStackElement.value?.getBoundingClientRect().height;
  cancelPanelResize();
  const generation = resizeGeneration;
  if (startHeight && startHeight > 0) panelHeight.value = startHeight;
  panelDirection.value = direction;
  animatePanel.value = true;
  panelStack.value = nextStack;
  if (!startHeight || startHeight <= 0) return;

  void nextTick().then(() => {
    if (generation !== resizeGeneration) return;
    const targetHeight = panelElement.value?.scrollHeight;
    if (!targetHeight || Math.abs(targetHeight - startHeight) < .5) {
      panelHeight.value = undefined;
      return;
    }
    resizeFrame = requestAnimationFrame(() => {
      if (generation !== resizeGeneration) return;
      resizeFrame = undefined;
      resizingPanel.value = true;
      panelHeight.value = targetHeight;
      resizeTimer = window.setTimeout(() => finishPanelResize(), resizeFallbackMs);
    });
  });
}

function finishPanelResize(): void {
  if (!resizingPanel.value) return;
  if (resizeTimer !== undefined) window.clearTimeout(resizeTimer);
  resizeTimer = undefined;
  resizingPanel.value = false;
  panelHeight.value = undefined;
}

function pushPanel(panel: string): void {
  if (!panel || panel === activePanel.value) return;
  transitionPanel([...panelStack.value, panel], "forward");
}

function popPanel(): void {
  if (panelStack.value.length <= 1) return;
  transitionPanel(panelStack.value.slice(0, -1), "backward");
}

function dismiss(): void {
  if (panelStack.value.length > 1) popPanel();
  else close();
}

onClickOutside(root, close);
useBackDismiss(() => open.value, dismiss);
onBeforeUnmount(cancelPanelResize);
</script>

<template>
  <div ref="root" class="anchored-dropdown" :data-open="open">
    <slot name="trigger" :open="open" :toggle="toggle" :close="close" />
    <div
      v-if="open"
      class="anchored-dropdown__menu"
      :class="[menuClass, `anchored-dropdown__menu--${align}`]"
      :style="menuStyle"
      role="menu"
      :aria-label="menuLabel"
      :data-panel="activePanel"
      @keydown.esc.stop.prevent="dismiss"
    >
      <div ref="panelStackElement" class="anchored-dropdown__panel-stack" :data-resizing="resizingPanel" :style="panelStackStyle" @transitionend.self="finishPanelResize">
        <div ref="panelElement" :key="activePanel" class="anchored-dropdown__panel" :data-animate="animatePanel" :data-direction="panelDirection">
          <slot :active-panel="activePanel" :panel-depth="panelStack.length - 1" :push-panel="pushPanel" :pop-panel="popPanel" :close="close" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.anchored-dropdown {
  position: relative;
  min-width: 0;
}

.anchored-dropdown[data-open="true"] {
  z-index: 50;
}

.anchored-dropdown__menu {
  position: absolute;
  max-width: calc(100vw - 32px);
  overflow: hidden;
  border: 1px solid var(--nh-border);
  border-radius: 6px;
  background: var(--nh-surface);
  box-shadow: var(--shadow-2);
  animation: anchored-dropdown-menu-enter .3s cubic-bezier(.2, .8, .2, 1) both;
}

.anchored-dropdown__menu--start {
  left: 0;
  transform-origin: top left;
}

.anchored-dropdown__menu--end {
  right: 0;
  transform-origin: top right;
}

.anchored-dropdown__panel-stack {
  display: grid;
  overflow: hidden;
  transition: height .3s cubic-bezier(.2, .8, .2, 1);
}

.anchored-dropdown__panel {
  min-width: 0;
  align-self: start;
  grid-area: 1 / 1;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__section) {
  padding: 4px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__section--divided) {
  position: relative;
  padding-top: 6px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__section--divided::before) {
  position: absolute;
  top: 0;
  right: 8px;
  left: 8px;
  border-top: 1px solid var(--nh-border);
  content: "";
}

.anchored-dropdown__menu :deep(.anchored-dropdown__section-title) {
  padding: 3px 8px 1px;
  color: var(--nh-muted);
  font-size: 11px;
  font-weight: 500;
  line-height: 17px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__options) {
  display: grid;
  padding: 4px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__section > .anchored-dropdown__options) {
  padding: 0;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option) {
  display: flex;
  width: 100%;
  min-height: 36px;
  align-items: center;
  justify-content: space-between;
  padding: 0 8px;
  border: 0;
  border-radius: 4px;
  color: var(--nh-text);
  background: transparent;
  font-size: 12px;
  text-align: left;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option:active),
.anchored-dropdown__menu :deep(.anchored-dropdown__option[data-selected="true"]) {
  color: var(--nh-selection);
  background: color-mix(in srgb, var(--nh-selection) 9%, var(--nh-surface));
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option[data-selected="true"]) {
  font-weight: 600;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option:disabled) {
  color: var(--nh-muted);
  background: transparent;
  opacity: .5;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-icon) {
  flex: 0 0 auto;
  color: var(--nh-selection);
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-icon[data-visible="false"]) {
  opacity: 0;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-content),
.anchored-dropdown__menu :deep(.anchored-dropdown__option-trailing) {
  display: flex;
  min-width: 0;
  align-items: center;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-content) {
  gap: 8px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-content > span) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-leading-icon) {
  flex: 0 0 auto;
  color: var(--nh-muted);
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option-trailing) {
  flex: 0 0 auto;
  color: var(--nh-muted);
  font-size: 11px;
  gap: 2px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option[data-tone="danger"]:not(:disabled)) {
  color: var(--nh-danger);
}

.anchored-dropdown__menu :deep(.anchored-dropdown__option[data-tone="danger"] .anchored-dropdown__option-leading-icon) {
  color: currentColor;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__row) {
  display: flex;
  min-height: 44px;
  align-items: center;
  justify-content: space-between;
  padding: 4px 8px;
  gap: 12px;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__row-label) {
  flex: 0 0 auto;
  font-size: 12px;
  font-weight: 500;
}

.anchored-dropdown__menu :deep(.anchored-dropdown__hint) {
  margin: -1px 8px 5px;
  color: var(--nh-muted);
  font-size: 11px;
  line-height: 18px;
}

.anchored-dropdown__panel[data-animate="true"][data-direction="forward"] {
  animation: anchored-dropdown-panel-forward-enter .3s cubic-bezier(.2, .8, .2, 1) both;
}

.anchored-dropdown__panel[data-animate="true"][data-direction="backward"] {
  animation: anchored-dropdown-panel-backward-enter .3s cubic-bezier(.2, .8, .2, 1) both;
}

@keyframes anchored-dropdown-panel-forward-enter {
  from {
    transform: translateX(20px);
  }

  to {
    transform: none;
  }
}

@keyframes anchored-dropdown-panel-backward-enter {
  from {
    transform: translateX(-20px);
  }

  to {
    transform: none;
  }
}

@keyframes anchored-dropdown-menu-enter {
  from {
    opacity: 0;
    transform: scale(.94) translateY(-5px);
  }

  to {
    opacity: 1;
    transform: none;
  }
}

@media (prefers-reduced-motion: reduce) {
  .anchored-dropdown__menu {
    animation: none;
  }

  .anchored-dropdown__panel[data-animate="true"] {
    animation: none;
  }

  .anchored-dropdown__panel-stack {
    transition: none;
  }
}
</style>
