<script setup lang="ts">
import { computed, nextTick, ref, useAttrs, useId, useSlots, watch } from "vue";
import { IconX } from "@tabler/icons-vue";
import IconButton from "../primitives/IconButton.vue";
import { useOverlayRuntime, type OverlayDismissReason } from "./useOverlayRuntime";

export type DialogDismissReason = OverlayDismissReason | "backdrop" | "action";
export type BeforeDismiss = (reason: DialogDismissReason) => boolean | Promise<boolean>;

const props = withDefaults(defineProps<{
  modelValue: boolean;
  title?: string;
  ariaLabel?: string;
  ariaDescribedby?: string;
  closeOnBackdrop?: boolean;
  closeOnEscape?: boolean;
  destroyOnClose?: boolean;
  beforeDismiss?: BeforeDismiss;
  initialFocus?: "first-safe-control" | "dialog";
  showCloseButton?: boolean;
  closeLabel?: string;
}>(), {
  title: "",
  ariaLabel: "",
  ariaDescribedby: "",
  closeOnBackdrop: false,
  closeOnEscape: true,
  destroyOnClose: true,
  initialFocus: "first-safe-control",
  showCloseButton: false,
  closeLabel: "关闭对话框",
});

defineOptions({ inheritAttrs: false });

const attrs = useAttrs();
const slots = useSlots();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  "dismiss-request": [reason: DialogDismissReason];
  opened: [];
  closed: [];
}>();

const panel = ref<HTMLElement>();
const shouldRender = ref(props.modelValue);
const phase = ref<"closed" | "enter" | "open" | "exit">(props.modelValue ? "enter" : "closed");
const dismissing = ref(false);
const uid = useId();
const hasTitle = computed(() => Boolean(props.title || slots.title));
const titleId = computed(() => hasTitle.value ? `nh-dialog-${uid}-title` : undefined);
const dialogLabel = computed(() => props.ariaLabel || (typeof attrs["aria-label"] === "string" ? attrs["aria-label"] : undefined));
const describedBy = computed(() => props.ariaDescribedby || (typeof attrs["aria-describedby"] === "string" ? attrs["aria-describedby"] : undefined));

function focusableElements(): HTMLElement[] {
  return Array.from(panel.value?.querySelectorAll<HTMLElement>('button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])') ?? [])
    .filter((element) => element.getAttribute("aria-hidden") !== "true");
}

function focusInitial(): void {
  if (!panel.value) return;
  if (props.initialFocus === "dialog") {
    panel.value.focus({ preventScroll: true });
    return;
  }
  const explicit = panel.value.querySelector<HTMLElement>('[data-dialog-autofocus="true"]');
  const controls = focusableElements();
  const safe = controls.find((element) => !element.matches("[data-dialog-danger], [data-danger], .nh-button--danger"));
  (explicit || safe || controls[0] || panel.value).focus({ preventScroll: true });
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key !== "Tab") return;
  const controls = focusableElements();
  if (controls.length === 0) {
    event.preventDefault();
    panel.value?.focus({ preventScroll: true });
    return;
  }
  const currentIndex = controls.indexOf(document.activeElement as HTMLElement);
  const nextIndex = event.shiftKey
    ? (currentIndex <= 0 ? controls.length - 1 : currentIndex - 1)
    : (currentIndex === controls.length - 1 ? 0 : currentIndex + 1);
  if (currentIndex === -1 || (event.shiftKey && currentIndex <= 0) || (!event.shiftKey && currentIndex === controls.length - 1)) {
    event.preventDefault();
    controls[nextIndex]?.focus({ preventScroll: true });
  }
}

async function requestDismiss(reason: DialogDismissReason = "action"): Promise<void> {
  if (!props.modelValue || dismissing.value) return;
  if (reason === "backdrop" && !props.closeOnBackdrop) return;
  if (reason === "escape" && !props.closeOnEscape) return;

  dismissing.value = true;
  emit("dismiss-request", reason);
  let allowed = true;
  if (props.beforeDismiss) {
    try { allowed = await props.beforeDismiss(reason); }
    catch { allowed = false; }
  }
  if (!allowed || !props.modelValue) {
    dismissing.value = false;
    return;
  }
  emit("update:modelValue", false);
}

const completeOverlayClose = useOverlayRuntime(
  () => props.modelValue,
  (reason) => { void requestDismiss(reason ?? "escape"); },
  panel,
  { deferCloseCleanup: true, type: "dialog", escapeDismissible: () => props.closeOnEscape },
);

watch(() => props.modelValue, (open) => {
  if (open) {
    shouldRender.value = true;
    phase.value = "enter";
    return;
  }
  if (shouldRender.value) phase.value = "exit";
  else completeOverlayClose();
});

function handleAnimationEnd(event: AnimationEvent): void {
  if (event.target !== panel.value) return;
  if (phase.value === "enter") {
    phase.value = "open";
    void nextTick(() => focusInitial());
    emit("opened");
  } else if (phase.value === "exit") {
    phase.value = "closed";
    if (props.destroyOnClose) shouldRender.value = false;
    completeOverlayClose();
    dismissing.value = false;
    emit("closed");
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="shouldRender"
      v-bind="attrs"
      class="nh-dialog"
      data-overlay-type="dialog"
      :class="[`nh-dialog--${phase}`, { 'nh-dialog--inactive': phase === 'exit' || phase === 'closed' }]"
    >
      <div class="nh-dialog__mask" aria-hidden="true" @pointerdown="requestDismiss('backdrop')"></div>
      <div class="nh-dialog__wrap" @pointerdown.self="requestDismiss('backdrop')">
        <section
          ref="panel"
          class="nh-dialog__panel"
          role="dialog"
          aria-modal="true"
          :aria-label="dialogLabel"
          :aria-labelledby="titleId"
          :aria-describedby="describedBy"
          tabindex="-1"
          @animationend="handleAnimationEnd"
          @keydown.capture="handleKeydown"
        >
            <header v-if="hasTitle || showCloseButton" class="nh-dialog__header">
              <h2 v-if="hasTitle" :id="titleId"><slot name="title">{{ title }}</slot></h2>
              <IconButton v-if="showCloseButton" class="nh-dialog__close" variant="text" size="s" :aria-label="closeLabel" :disabled="dismissing" @click="requestDismiss('action')">
                <IconX :size="18" aria-hidden="true" />
              </IconButton>
            </header>
            <div class="nh-dialog__body"><slot /></div>
            <footer v-if="$slots.actions" class="nh-dialog__actions">
              <slot name="actions" :request-close="requestDismiss" :dismissing="dismissing" />
            </footer>
        </section>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.nh-dialog { position: fixed; z-index: var(--overlay-z-dialog, 1000); inset: 0; pointer-events: auto; }
.nh-dialog--inactive { pointer-events: none; }
.nh-dialog--closed { visibility: hidden; }
.nh-dialog__mask { position: absolute; inset: 0; background: var(--scrim-default); opacity: 1; backdrop-filter: blur(2px); }
.nh-dialog__wrap { position: absolute; inset: 0; display: grid; padding: max(16px, env(safe-area-inset-top)) 16px max(16px, env(safe-area-inset-bottom)); place-items: center; pointer-events: none; }
.nh-dialog__panel { display: flex; width: min(100%, 420px); max-height: min(var(--visual-viewport-height, 82dvh), 640px); flex-direction: column; overflow: hidden; border: 1px solid var(--border-default); border-radius: var(--radius-xl, 12px); outline: 0; color: var(--text-primary); background: var(--surface); box-shadow: var(--shadow-3); }
.nh-dialog__panel { pointer-events: auto; transform-origin: center center; will-change: transform, opacity; }
.nh-dialog__header { display: flex; min-height: 52px; align-items: center; justify-content: space-between; padding: 16px 18px 8px; gap: 8px; }
.nh-dialog__header h2 { min-width: 0; margin: 0; font-size: 17px; line-height: 1.35; }
.nh-dialog__close { flex: 0 0 auto; }
.nh-dialog__body { min-height: 0; overflow: auto; padding: 8px 18px 18px; color: var(--text-secondary); font-size: 13px; line-height: 1.55; overscroll-behavior: contain; }
.nh-dialog__actions { display: flex; flex: 0 0 auto; justify-content: flex-end; padding: 12px 18px max(18px, env(safe-area-inset-bottom)); border-top: 1px solid var(--border-divider); gap: 8px; }
.nh-dialog--enter .nh-dialog__mask { animation: nh-dialog-mask-fade-in .46s ease both; }
.nh-dialog--exit .nh-dialog__mask { animation: nh-dialog-mask-fade-out .345s ease both; }
.nh-dialog--enter .nh-dialog__panel { animation: nh-dialog-fade-in .46s cubic-bezier(.22,.61,.36,1) both; }
.nh-dialog--exit .nh-dialog__panel { animation: nh-dialog-fade-out .345s cubic-bezier(.22,.61,.36,1) both; }
@keyframes nh-dialog-mask-fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes nh-dialog-mask-fade-out { from { opacity: 1; } to { opacity: 0; } }
@keyframes nh-dialog-fade-in { from { opacity: 0; transform: scale(.96); } to { opacity: 1; transform: none; } }
@keyframes nh-dialog-fade-out { from { opacity: 1; transform: scale(1); } to { opacity: 0; transform: scale(.96); } }
@media (max-width: 420px) { .nh-dialog__actions { flex-direction: column-reverse; } .nh-dialog__actions :deep(.nh-button) { width: 100%; } }
@media (prefers-reduced-motion: reduce) { .nh-dialog--enter .nh-dialog__mask, .nh-dialog--exit .nh-dialog__mask, .nh-dialog--enter .nh-dialog__panel, .nh-dialog--exit .nh-dialog__panel { animation-duration: .01ms !important; animation-iteration-count: 1 !important; } }
</style>
