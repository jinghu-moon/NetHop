<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import { IconActivity, IconApps, IconSettings, IconWorld } from "@tabler/icons-vue";
import { createAppHost, provideHost } from "@/bridge/context";
import { compatibilityMessage, compatibilityState } from "./compatibility";
import { uiStores } from "@/runtime/store";
import { EventSession } from "@/runtime/event-session";
import { useEventLifecycle } from "@/runtime/use-event-lifecycle";
import { parseEventFrame, parseStatus } from "@/model/dto";
import { publishLiveTraffic } from "@/runtime/live-store";
import { applyRuntimeEvent } from "@/runtime/event-state";
import { validatedQuery } from "@/model/client";
import { useTheme } from "./theme";
import { useBackDispatcher } from "./useBackDispatcher";
import { useKeyboardViewport } from "./useKeyboardViewport";
import Button from "@/components/ui/primitives/Button.vue";
import NoticeBar from "@/components/ui/feedback/NoticeBar.vue";
import TabBar from "@/components/ui/navigation/TabBar.vue";

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
      applyRuntimeEvent(frame.payload);
      if (frame.payload.kind === "runtime") void validatedQuery(appHost, { id: "status.get" }, parseStatus).then((status) => uiStores.session.setStatus(status)).catch(() => uiStores.session.setPhase("stale"));
    },
  });
  useEventLifecycle(session);
}

const nav = [
  { value: "/overview", label: "概览", icon: IconActivity },
  { value: "/subscriptions", label: "订阅", icon: IconWorld },
  { value: "/applications", label: "应用", icon: IconApps },
  { value: "/settings", label: "设置", icon: IconSettings },
] as const;
const activeNav = computed(() => nav.find((item) => route.path.startsWith(item.value))?.value ?? route.path);
const navigate = (value: string): void => {
  if (nav.some((item) => item.value === value)) void router.push(value);
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
    <NoticeBar v-if="compatibility !== 'ready'" :content="compatibilityMessage(compatibility)"><template #action><Button size="s" variant="primary" @click="reloadUi">重新加载</Button></template></NoticeBar>
    <NoticeBar v-else-if="phase === 'stale'" content="运行状态已过期，危险操作已暂时禁用"><template #action><Button size="s" variant="primary" @click="reloadUi">重新加载</Button></template></NoticeBar>
    <main class="app-content"><RouterView v-slot="{ Component }"><KeepAlive :max="6"><component :is="Component" /></KeepAlive></RouterView></main>

    <TabBar v-show="!keyboard.visible.value" :model-value="activeNav" :items="nav" @change="navigate" />
  </div>
</template>
