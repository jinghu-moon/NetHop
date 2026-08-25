import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";
import { uiStores } from "./runtime/store";

const routes: RouteRecordRaw[] = [
  { path: "/", redirect: "/overview" },
  { path: "/overview", name: "overview", component: () => import("./views/OverviewView.vue") },
  { path: "/subscriptions", name: "subscriptions", component: () => import("./views/SubscriptionsView.vue") },
  { path: "/applications", name: "applications", component: () => import("./views/ApplicationsView.vue") },
  { path: "/settings", name: "settings", component: () => import("./views/SettingsView.vue") },
  { path: "/settings/updates", name: "settings-updates", component: () => import("./views/SettingsView.vue"), meta: { settingsSection: "updates" } },
  { path: "/settings/network", name: "settings-network", component: () => import("./views/SettingsView.vue"), meta: { settingsSection: "network" } },
  { path: "/settings/interfaces", name: "settings-interfaces", component: () => import("./views/SettingsView.vue"), meta: { settingsSection: "interfaces" } },
  { path: "/settings/routing", name: "settings-routing", component: () => import("./views/SettingsView.vue"), meta: { settingsSection: "routing" } },
  { path: "/settings/logging", name: "settings-logging", component: () => import("./views/SettingsView.vue"), meta: { settingsSection: "logging" } },
  { path: "/settings/advanced", name: "settings-advanced", component: () => import("./views/SettingsView.vue"), meta: { settingsSection: "advanced" } },
  { path: "/nodes", name: "nodes", component: () => import("./views/NodesView.vue") },
  { path: "/operations", name: "operations", component: () => import("./views/OperationsView.vue") },
];

if (import.meta.env.VITE_ENABLE_UI_FOUNDATION === "true") {
  routes.push({ path: "/dev/ui-foundation", name: "ui-foundation", component: () => import("./views/UiFoundationView.vue") });
}

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

router.beforeEach(() => {
  if (!uiStores.config.dirty.value) return true;
  return window.confirm("存在尚未应用的配置修改，确认离开并保留草稿吗？");
});
