import { createRouter, createWebHashHistory } from "vue-router";
import { uiStores } from "./runtime/store";


export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/overview" },
    { path: "/overview", name: "overview", component: () => import("./views/OverviewView.vue") },
    { path: "/subscriptions", name: "subscriptions", component: () => import("./views/SubscriptionsView.vue") },
    { path: "/applications", name: "applications", component: () => import("./views/ApplicationsView.vue") },
    { path: "/settings", name: "settings", component: () => import("./views/SettingsView.vue") },
    { path: "/nodes", name: "nodes", component: () => import("./views/NodesView.vue") },
    { path: "/operations", name: "operations", component: () => import("./views/OperationsView.vue") },
  ],
});

router.beforeEach(() => {
  if (!uiStores.config.dirty.value) return true;
  return window.confirm("存在尚未应用的配置修改，确认离开并保留草稿吗？");
});
