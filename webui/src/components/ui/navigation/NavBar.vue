<script setup lang="ts">
import { computed, useAttrs } from "vue";
import { IconChevronLeft } from "@tabler/icons-vue";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  title?: string;
  titleMaxLength?: number;
  visible?: boolean;
  fixed?: boolean;
  placeholder?: boolean;
  safeAreaInsetTop?: boolean;
  animation?: boolean;
  leftArrow?: boolean;
  ariaLabel?: string;
}>(), {
  title: "",
  visible: true,
  fixed: false,
  placeholder: false,
  safeAreaInsetTop: true,
  animation: true,
  leftArrow: false,
  ariaLabel: "页面导航",
});

const attrs = useAttrs();
const emit = defineEmits<{ leftClick: []; rightClick: [] }>();

const displayTitle = computed(() => {
  if (!props.title || !props.titleMaxLength || props.titleMaxLength < 1 || props.title.length <= props.titleMaxLength) return props.title;
  return `${props.title.slice(0, Math.max(1, props.titleMaxLength - 1))}…`;
});
</script>

<template>
  <nav
    v-bind="attrs"
    class="nh-nav-bar"
    :class="{ 'nh-nav-bar--fixed': fixed, 'nh-nav-bar--safe': safeAreaInsetTop, 'nh-nav-bar--animated': animation, 'nh-nav-bar--hidden': !visible }"
    :aria-label="ariaLabel"
    :aria-hidden="!visible ? 'true' : undefined"
  >
    <div class="nh-nav-bar__inner">
      <div class="nh-nav-bar__left" @click="emit('leftClick')">
        <slot name="capsule" />
        <slot name="left">
          <button v-if="leftArrow" class="nh-nav-bar__back" type="button" aria-label="返回" @click.stop="emit('leftClick')">
            <IconChevronLeft :size="22" stroke-width="1.9" aria-hidden="true" />
          </button>
        </slot>
      </div>
      <div class="nh-nav-bar__title" :title="displayTitle">
        <slot name="title">{{ displayTitle }}</slot>
      </div>
      <div class="nh-nav-bar__right" @click="emit('rightClick')"><slot name="right" /></div>
    </div>
  </nav>
  <div v-if="placeholder && fixed" class="nh-nav-bar__placeholder" :class="{ 'nh-nav-bar__placeholder--safe': safeAreaInsetTop }" aria-hidden="true" />
</template>

<style scoped>
.nh-nav-bar {
  --nav-bar-height: 48px;
  --nav-bar-safe: env(safe-area-inset-top, 0px);
  position: relative;
  z-index: var(--overlay-z-navbar, 90);
  width: 100%;
  min-height: var(--nav-bar-height);
  box-sizing: border-box;
  border-bottom: 1px solid var(--border-divider);
  color: var(--text-primary);
  background: var(--surface);
  transition: opacity .16s ease, transform .18s ease;
}
.nh-nav-bar--fixed { position: fixed; top: 0; right: 0; left: 0; }
.nh-nav-bar--safe { min-height: calc(var(--nav-bar-height) + var(--nav-bar-safe)); padding-top: var(--nav-bar-safe); }
.nh-nav-bar--hidden { opacity: 0; pointer-events: none; transform: translateY(-8px); }
.nh-nav-bar:not(.nh-nav-bar--animated) { transition: none; }
.nh-nav-bar__inner { display: grid; width: min(100%, 820px); min-height: var(--nav-bar-height); margin: 0 auto; padding: 0 12px; box-sizing: border-box; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr); align-items: center; gap: 8px; }
.nh-nav-bar--safe .nh-nav-bar__inner { min-height: var(--nav-bar-height); }
.nh-nav-bar__left, .nh-nav-bar__right { display: flex; min-width: 0; min-height: 44px; align-items: center; gap: 4px; }
.nh-nav-bar__left { justify-content: flex-start; }
.nh-nav-bar__right { justify-content: flex-end; }
.nh-nav-bar__title { min-width: 0; max-width: min(60vw, 420px); overflow: hidden; font-size: 17px; font-weight: 600; line-height: 1.25; text-align: center; text-overflow: ellipsis; white-space: nowrap; }
.nh-nav-bar__back { display: inline-flex; width: 44px; height: 44px; align-items: center; justify-content: center; padding: 0; border: 0; border-radius: 8px; color: inherit; background: transparent; cursor: pointer; }
.nh-nav-bar__back:hover { background: var(--state-hover); }
.nh-nav-bar__back:active { background: var(--state-pressed); transform: scale(.96); }
.nh-nav-bar__back:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
.nh-nav-bar__left :deep(button), .nh-nav-bar__right :deep(button) { flex: 0 0 auto; }
.nh-nav-bar__placeholder { min-height: var(--nav-bar-height); }
.nh-nav-bar__placeholder--safe { min-height: calc(var(--nav-bar-height) + env(safe-area-inset-top, 0px)); }
@media (max-width: 420px) { .nh-nav-bar__inner { padding-inline: 8px; } .nh-nav-bar__title { max-width: 56vw; } }
@media (prefers-reduced-motion: reduce) { .nh-nav-bar { transition: none; } .nh-nav-bar__back:active { transform: none; } }
</style>
