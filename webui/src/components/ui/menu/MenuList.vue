<script setup lang="ts">
import { computed, onBeforeUnmount, provide, ref, useAttrs } from "vue";
import { menuContextKey, type MenuSemantic } from "./menu-context";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  semantic?: MenuSemantic;
  modelValue?: string;
  disabled?: boolean;
  loop?: boolean;
}>(), {
  semantic: "menu",
  disabled: false,
  loop: true,
});

const attrs = useAttrs();
const root = ref<HTMLElement>();
const activeValue = ref<string>();
const typeahead = ref("");
let typeaheadTimer: ReturnType<typeof setTimeout> | undefined;

const semantic = computed(() => props.semantic);
const modelValue = computed(() => props.modelValue);
const disabled = computed(() => props.disabled);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  select: [value: string | undefined, event?: Event];
}>();

function select(value: string | undefined, event?: Event): void {
  if (disabled.value || value === undefined) {
    if (value === undefined) emit("select", value, event);
    return;
  }
  emit("update:modelValue", value);
  emit("select", value, event);
  activeValue.value = value;
}

provide(menuContextKey, { semantic, modelValue, disabled, activeValue, select });

function items(): HTMLElement[] {
  return Array.from(root.value?.querySelectorAll<HTMLElement>("[data-menu-item='true']") ?? [])
    .filter((item) => item.getAttribute("aria-disabled") !== "true" && item.dataset.divider !== "true");
}

function focusItem(item: HTMLElement | undefined): void {
  if (!item) return;
  item.focus({ preventScroll: true });
  activeValue.value = item.dataset.value || undefined;
}

function handleKeydown(event: KeyboardEvent): void {
  const navigable = items();
  if (navigable.length === 0) return;
  const current = document.activeElement instanceof HTMLElement ? navigable.indexOf(document.activeElement) : -1;
  const currentIndex = current >= 0 ? current : 0;

  if (["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft", "Home", "End"].includes(event.key)) {
    event.preventDefault();
    let nextIndex = currentIndex;
    if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = navigable.length - 1;
    else if (event.key === "ArrowDown" || event.key === "ArrowRight") nextIndex = props.loop ? (currentIndex + 1) % navigable.length : Math.min(currentIndex + 1, navigable.length - 1);
    else nextIndex = props.loop ? (currentIndex - 1 + navigable.length) % navigable.length : Math.max(currentIndex - 1, 0);
    focusItem(navigable[nextIndex]);
    return;
  }

  if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
    typeahead.value += event.key.toLocaleLowerCase();
    if (typeaheadTimer) clearTimeout(typeaheadTimer);
    typeaheadTimer = setTimeout(() => { typeahead.value = ""; }, 500);
    const match = navigable.find((item) => (item.textContent || "").trim().toLocaleLowerCase().startsWith(typeahead.value));
    if (match) {
      event.preventDefault();
      focusItem(match);
    }
  }
}

onBeforeUnmount(() => { if (typeaheadTimer) clearTimeout(typeaheadTimer); });
</script>

<template>
  <div
    ref="root"
    v-bind="attrs"
    class="nh-menu-list"
    :class="[`nh-menu-list--${semantic}`, { 'nh-menu-list--disabled': disabled }]"
    :role="semantic"
    :aria-label="typeof attrs['aria-label'] === 'string' ? attrs['aria-label'] : undefined"
    :aria-disabled="disabled ? 'true' : undefined"
    @keydown="handleKeydown"
  >
    <slot />
  </div>
</template>

<style scoped>
.nh-menu-list { display: grid; min-width: 0; gap: 2px; padding: 4px; color: var(--text-primary); }
.nh-menu-list--listbox { gap: 3px; }
.nh-menu-list--disabled { pointer-events: none; opacity: .58; }
</style>
