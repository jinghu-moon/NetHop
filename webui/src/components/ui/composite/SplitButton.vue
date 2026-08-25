<script setup lang="ts">
import { ref } from "vue";
import { IconChevronDown } from "@tabler/icons-vue";
import Button from "../primitives/Button.vue";
import IconButton from "../primitives/IconButton.vue";
import Dropdown, { type DropdownPlacement } from "../overlay/Dropdown.vue";

type ButtonVariant = "default" | "primary" | "danger" | "outline" | "text";
type ButtonSize = "s" | "m" | "l";

const props = withDefaults(defineProps<{
  variant?: ButtonVariant;
  size?: ButtonSize;
  label: string;
  disabled?: boolean;
  loading?: boolean;
  menuPlacement?: DropdownPlacement;
}>(), {
  variant: "primary",
  size: "m",
  disabled: false,
  loading: false,
  menuPlacement: "bottom-end",
});

const emit = defineEmits<{ click: [event: MouseEvent] }>();
const menuOpen = ref(false);
</script>

<template>
  <div class="nh-split-button" :class="[`nh-split-button--${size}`, { 'nh-split-button--disabled': disabled }]">
    <Button class="nh-split-button__main" :variant="variant" :size="size" :disabled="disabled" :loading="loading" @click="emit('click', $event)">
      <slot>{{ label }}</slot>
    </Button>
    <Dropdown v-model:open="menuOpen" :placement="menuPlacement" :disabled="disabled">
      <template #trigger="{ open }">
        <IconButton class="nh-split-button__toggle" :variant="variant" :size="size" :disabled="disabled" :aria-label="`${label}更多操作`" :aria-expanded="open">
          <IconChevronDown :size="16" aria-hidden="true" />
        </IconButton>
      </template>
      <template #default="{ close, select }"><slot name="menu" :close="close" :select="select" /></template>
    </Dropdown>
  </div>
</template>

<style scoped>
.nh-split-button { display: inline-flex; align-items: stretch; isolation: isolate; }
.nh-split-button :deep(.nh-button) { border-radius: 0; }
.nh-split-button__main { border-top-left-radius: var(--button-radius, 6px) !important; border-bottom-left-radius: var(--button-radius, 6px) !important; }
.nh-split-button__toggle { margin-left: -1px; border-top-right-radius: var(--button-radius, 6px) !important; border-bottom-right-radius: var(--button-radius, 6px) !important; }
.nh-split-button--disabled { opacity: .7; }
</style>
