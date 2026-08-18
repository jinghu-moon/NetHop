import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-vue";
import { nextTick } from "vue";

import AnchoredDropdown from "@/components/AnchoredDropdown.vue";
import { dispatchBack } from "@/shell/useBackDispatcher";
import StatusLine from "@/components/StatusLine.vue";
import MetricValue from "@/components/MetricValue.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/PageState.vue";
import SchemaField from "@/components/SchemaField.vue";
import TrafficSparkline from "@/components/TrafficSparkline.vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";

describe("shared WebUI components", () => {
  it("removes an anchored menu immediately when an outside click closes it", async () => {
    const screen = render({
      components: { AnchoredDropdown },
      template: `<div><AnchoredDropdown menu-label="测试菜单"><template #trigger="{ toggle }"><button type="button" @click="toggle">打开菜单</button></template><span>菜单内容</span></AnchoredDropdown><button type="button">菜单外</button></div>`,
    });

    await screen.getByText("打开菜单", { exact: true }).click();
    const menu = document.querySelector<HTMLElement>(".anchored-dropdown__menu");
    expect(menu).not.toBeNull();
    expect(getComputedStyle(menu as HTMLElement).animationName).toContain("anchored-dropdown-menu-enter");
    expect(getComputedStyle(menu as HTMLElement).animationDuration).toBe("0.3s");

    await screen.getByText("菜单外", { exact: true }).click();
    expect(document.querySelector(".anchored-dropdown__menu")).toBeNull();
  });

  it("returns through anchored menu panels before closing the menu", async () => {
    const screen = render({
      components: { AnchoredDropdown },
      template: `<AnchoredDropdown menu-label="分级菜单"><template #trigger="{ toggle }"><button type="button" @click="toggle">打开分级菜单</button></template><template #default="{ activePanel, pushPanel, popPanel }"><button v-if="activePanel === 'root'" type="button" @click="pushPanel('sort')">进入排序</button><div v-else style="height:80px"><button type="button" @click="popPanel">返回操作</button><span>二级内容</span></div></template></AnchoredDropdown>`,
    });

    await screen.getByText("打开分级菜单", { exact: true }).click();
    await screen.getByText("进入排序", { exact: true }).click();
    expect(document.querySelector(".anchored-dropdown__menu")?.getAttribute("data-panel")).toBe("sort");
    expect(document.querySelectorAll(".anchored-dropdown__panel")).toHaveLength(1);
    expect(document.body.textContent).not.toContain("进入排序");
    await nextTick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    await nextTick();
    const panelStack = document.querySelector<HTMLElement>(".anchored-dropdown__panel-stack");
    expect(panelStack?.dataset.resizing).toBe("true");
    expect(panelStack?.style.height).not.toBe("");
    expect(getComputedStyle(panelStack as HTMLElement).transitionProperty).toContain("height");
    expect(getComputedStyle(panelStack as HTMLElement).transitionDuration).toBe("0.3s");
    expect(getComputedStyle(document.querySelector<HTMLElement>(".anchored-dropdown__panel") as HTMLElement).animationDuration).toBe("0.3s");

    expect(dispatchBack()).toBe(true);
    await expect.element(screen.getByText("进入排序", { exact: true })).toBeVisible();
    expect(document.querySelector(".anchored-dropdown__menu")?.getAttribute("data-panel")).toBe("root");
    expect(document.querySelectorAll(".anchored-dropdown__panel")).toHaveLength(1);
    expect(document.body.textContent).not.toContain("返回操作");

    expect(dispatchBack()).toBe(true);
    await nextTick();
    await expect.poll(() => document.querySelector(".anchored-dropdown__menu")).toBeNull();
  });

  it("renders status, metrics and operation states", async () => {
    const screen = render({ components: { StatusLine, MetricValue, OperationBanner }, template: `<StatusLine status="degraded" label="网络降级"/><MetricValue label="下行" :value="1024" unit="B/s"/><OperationBanner phase="failure" message="更新失败"/>` });
    await expect.element(screen.getByText("网络降级")).toBeVisible();
    await expect.element(screen.getByText("1024")).toBeVisible();
    await expect.element(screen.getByText("更新失败")).toBeVisible();
    expect(document.querySelector(".t-notice-bar")).toBeNull();
  });

  it("always gives a running operation a visible message", async () => {
    const screen = render({ components: { OperationBanner }, template: `<OperationBanner phase="running"/>` });
    await expect.element(screen.getByText("正在处理中，请稍候")).toBeVisible();
    const spinner = document.querySelector<SVGElement>(".operation-message__spinner");
    expect(spinner).not.toBeNull();
    expect(getComputedStyle(spinner as SVGElement).animationName).toContain("operation-message-spin");
  });

  it("renders bounded schema, empty state, canvas and virtual rows", async () => {
    const items = Array.from({ length: 100 }, (_, index) => ({ id: `item-${index}`, label: `Item ${index}` }));
    const screen = render({ components: { PageState, SchemaField, TrafficSparkline, VirtualListViewport }, data: () => ({ items }), template: `<PageState kind="empty" title="没有内容"/><SchemaField :field="{ id:'enabled', label:'启用', valueType:'bool', value:true }"/><TrafficSparkline :points="[{up:1,down:2,state:'ok',intervalMs:1000,observedAtMs:1,timestampMs:1},{up:2,down:4,state:'ok',intervalMs:1000,observedAtMs:2,timestampMs:2}]"/><VirtualListViewport :items="items" :get-item-key="(_i, item) => item.id"><template #default="{ item }"><span>{{ item.label }}</span></template></VirtualListViewport>` });
    await expect.element(screen.getByText("没有内容")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".t-empty")).toBeVisible();
    await expect.element(document.querySelector<HTMLCanvasElement>(".sparkline-wrap canvas")).toBeVisible();
    await expect.element(screen.getByText("5", { exact: true })).toBeVisible();
    await expect.element(screen.getByText("2.5", { exact: true })).toBeVisible();
    expect(document.querySelector(".sparkline-wrap canvas")?.getAttribute("aria-label")).toContain("0 至 5 B/s");
    await expect.element(document.querySelector<HTMLElement>(".virtual-viewport")).toBeVisible();
  });

  it("delegates loading and error states to TDesign", async () => {
    const screen = render({ components: { PageState }, template: `<PageState kind="loading" title="正在加载"/><PageState kind="error" title="加载失败" detail="稍后重试"/>` });
    await expect.element(screen.getByText("正在加载")).toBeVisible();
    await expect.element(screen.getByText("加载失败")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".t-loading")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".t-result")).toBeVisible();
  });
});
