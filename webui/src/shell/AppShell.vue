<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import { IconActivity, IconApps, IconSettings, IconWorld } from "@tabler/icons-vue";
import { Button as TButton, NoticeBar as TNoticeBar, TabBar as TTabBar, TabBarItem as TTabBarItem } from "tdesign-mobile-vue";
import { createAppHost, provideHost } from "@/bridge/context";
import { compatibilityMessage, compatibilityState } from "./compatibility";
import { uiStores } from "@/runtime/store";
import { EventSession } from "@/runtime/event-session";
import { useEventLifecycle } from "@/runtime/use-event-lifecycle";
import { parseEventFrame, parseStatus } from "@/model/dto";
import { publishLiveTraffic } from "@/runtime/live-store";
import { validatedQuery } from "@/model/client";
import { useTheme } from "./theme";
import { useBackDispatcher } from "./useBackDispatcher";
import { useKeyboardViewport } from "./useKeyboardViewport";

const route = useRoute();
const router = useRouter();
const appHost = createAppHost();
provideHost(appHost);
useTheme();
useBackDispatcher();
const keyboard = useKeyboardViewport();
const host = ref(appHost.capability);
const phase = computed(() => uiStores.session.phase.value);
const compatibility = computed(() => compatibilityState(host.value, uiStores.session.hello.value));
if (appHost.capability.kind !== "browser") {
  const session = new EventSession({
    host: appHost,
    kinds: ["traffic"],
    managerVersion: "0.1.0",
    onState: (state) => uiStores.session.setPhase(state.stale ? "stale" : "live"),
    onFrame: (raw) => {
      const frame = parseEventFrame(raw);
      if (frame.type !== "item") return;
      if (frame.payload.traffic) publishLiveTraffic(frame.payload.traffic);
      if (frame.payload.kind === "runtime") void validatedQuery(appHost, { id: "status.get" }, parseStatus).then((status) => uiStores.session.setStatus(status)).catch(() => uiStores.session.setPhase("stale"));
    },
  });
  useEventLifecycle(session);
}

const nav = [
  { to: "/overview", label: "概览", icon: IconActivity },
  { to: "/subscriptions", label: "订阅", icon: IconWorld },
  { to: "/applications", label: "应用", icon: IconApps },
  { to: "/settings", label: "设置", icon: IconSettings },
] as const;
const activeNav = computed(() => nav.find((item) => route.path.startsWith(item.to))?.to ?? route.path);
const navigate = (value: string | number | Array<string | number>): void => {
  if (typeof value === "string" && nav.some((item) => item.to === value)) void router.push(value);
};

onMounted(async () => {
  uiStores.session.setHost(host.value);
  uiStores.session.setPhase(host.value.available ? "connecting" : "unavailable");
  await new Promise((resolve) => setTimeout(resolve, 30));
  if (host.value.available) uiStores.session.setPhase("live");
});

const reloadUi = (): void => window.location.reload();
</script>

<template>
  <div class="app-shell" :data-phase="phase" :data-keyboard="keyboard.visible.value" :style="{ '--nh-visual-height': `${keyboard.viewportHeight.value}px` }">
    <TNoticeBar v-if="compatibility !== 'ready'" visible theme="warning" :content="compatibilityMessage(compatibility)" :marquee="false"><template #operation><TButton size="small" theme="primary" variant="text" @click="reloadUi">重新加载</TButton></template></TNoticeBar>
    <TNoticeBar v-else-if="phase === 'stale'" visible theme="warning" content="运行状态已过期，危险操作已暂时禁用" :marquee="false"><template #operation><TButton size="small" theme="primary" variant="text" @click="reloadUi">重新加载</TButton></template></TNoticeBar>
    <main class="app-content"><RouterView v-slot="{ Component }"><KeepAlive :max="6"><component :is="Component" /></KeepAlive></RouterView></main>

    <TTabBar v-show="!keyboard.visible.value" :value="activeNav" fixed placeholder :split="false" @change="navigate">
      <TTabBarItem v-for="item in nav" :key="item.to" :value="item.to"><template #icon><component :is="item.icon" :size="21" stroke-width="1.8" /></template>{{ item.label }}</TTabBarItem>
    </TTabBar>
  </div>
</template>
