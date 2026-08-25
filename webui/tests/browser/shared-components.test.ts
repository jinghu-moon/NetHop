import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-vue";
import { nextTick } from "vue";

import StatusLine from "@/components/StatusLine.vue";
import MetricValue from "@/components/MetricValue.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import Dropdown from "@/components/ui/overlay/Dropdown.vue";
import Dialog from "@/components/ui/overlay/Dialog.vue";
import PageState from "@/components/ui/feedback/PageState.vue";
import SettingsFieldControl from "@/components/settings/SettingsFieldControl.vue";
import MenuItemRadio from "@/components/ui/menu/MenuItemRadio.vue";
import Select from "@/components/ui/primitives/Select.vue";
import Disclosure from "@/components/ui/layout/Disclosure.vue";
import TrafficSparkline from "@/components/TrafficSparkline.vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";

describe("shared WebUI components", () => {
  it("removes an anchored menu immediately when an outside click closes it", async () => {
    const screen = render({
      components: { Dropdown },
      data: () => ({ open: false }),
      template: `<div><Dropdown v-model:open="open" aria-label="测试菜单"><template #trigger="{ toggle }"><button type="button" @click="toggle">打开菜单</button></template><span>菜单内容</span></Dropdown><button type="button">菜单外</button></div>`,
    });

    await screen.getByText("打开菜单", { exact: true }).click();
    const menu = document.querySelector<HTMLElement>(".nh-dropdown__panel");
    expect(menu).not.toBeNull();
    expect(document.querySelector(".nh-dropdown[data-overlay-type='dropdown']")).not.toBeNull();

    await screen.getByText("菜单外", { exact: true }).click();
    await expect.poll(() => document.querySelector(".nh-dropdown__panel")).toBeNull();
  });

  it("opens nested dropdown panels and closes through the trigger", async () => {
    const screen = render({
      components: { Dropdown },
      data: () => ({ open: false }),
      template: `<div><Dropdown v-model:open="open" aria-label="分级菜单"><template #trigger="{ toggle }"><button type="button" @click="toggle">打开分级菜单</button></template><template #default="{ activePanel, pushPanel, popPanel }"><button v-if="activePanel === 'root'" type="button" @click="pushPanel('sort')">进入排序</button><div v-else style="height:80px"><button type="button" @click="popPanel">返回操作</button><span>二级内容</span></div></template></Dropdown></div>`,
    });

    await screen.getByText("打开分级菜单", { exact: true }).click();
    const rootAction = document.querySelector<HTMLButtonElement>(".nh-dropdown__panel[data-panel='root'] button");
    expect(rootAction).not.toBeNull();
    rootAction?.click();
    await nextTick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    await nextTick();
    expect(document.querySelector(".nh-dropdown__panel")?.getAttribute("data-panel")).toBe("sort");
    expect(document.querySelectorAll(".nh-dropdown__content")).toHaveLength(1);
    expect(document.body.textContent).not.toContain("进入排序");
    await nextTick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    await nextTick();
    await screen.getByText("打开分级菜单", { exact: true }).click();
    await expect.poll(() => document.querySelector(".nh-dropdown__panel")).toBeNull();
  });

  it("renders status, metrics and operation states", async () => {
    const screen = render({ components: { StatusLine, MetricValue, OperationBanner }, template: `<StatusLine status="degraded" label="网络降级"/><MetricValue label="下行" :value="1024" unit="B/s"/><OperationBanner phase="failure" message="更新失败"/>` });
    await expect.element(screen.getByText("网络降级")).toBeVisible();
    await expect.element(screen.getByText("1024")).toBeVisible();
    await expect.element(screen.getByText("更新失败")).toBeVisible();
    expect(document.querySelector(".nh-notice-bar")).toBeNull();
  });

  it("renders actionable native confirmation buttons", async () => {
    const screen = render({ components: { Dialog }, data: () => ({ visible: true }), template: `<Dialog v-model="visible" title="应用配置" aria-label="应用配置"><p>确认应用</p><template #actions><button type="button">取消</button><button type="button">应用</button></template></Dialog>` });
    await expect.element(screen.getByText("应用配置", { exact: true })).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "取消" })).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "应用" })).toBeVisible();
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
    const screen = render({ components: { PageState, SettingsFieldControl, TrafficSparkline, VirtualListViewport }, data: () => ({ items }), template: `<PageState :model="{ type: 'empty', title: '没有内容' }"/><SettingsFieldControl :field="{ id:'enabled', label:'启用', valueType:'bool', value:true }"/><TrafficSparkline :points="[{up:1,down:2,state:'ok',intervalMs:1000,observedAtMs:1,timestampMs:1},{up:2,down:4,state:'ok',intervalMs:1000,observedAtMs:2,timestampMs:2}]"/><VirtualListViewport :items="items" :get-item-key="(_i, item) => item.id"><template #default="{ item }"><span>{{ item.label }}</span></template></VirtualListViewport>` });
    await expect.element(screen.getByText("没有内容")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".nh-page-state[data-state='empty']")).toBeVisible();
    await expect.element(document.querySelector<HTMLCanvasElement>(".sparkline-wrap canvas")).toBeVisible();
    await expect.element(screen.getByText("5", { exact: true })).toBeVisible();
    await expect.element(screen.getByText("2.5", { exact: true })).toBeVisible();
    expect(document.querySelector(".sparkline-wrap canvas")?.getAttribute("aria-label")).toContain("0 至 5 B/s");
    await expect.element(document.querySelector<HTMLElement>(".virtual-viewport")).toBeVisible();
  });

  it("renders loading and error states with native feedback semantics", async () => {
    const screen = render({ components: { PageState }, template: `<PageState :model="{ type: 'loading', title: '正在加载' }"/><PageState :model="{ type: 'error', title: '加载失败', detail: '稍后重试' }"/>` });
    await expect.element(screen.getByText("正在加载")).toBeVisible();
    await expect.element(screen.getByText("加载失败")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".nh-page-state[data-state='loading']")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".nh-page-state[data-state='error']")).toBeVisible();
  });

  it("provides menu radio, native select, and disclosure semantics", async () => {
    const screen = render({
      components: { MenuItemRadio, Select, Disclosure },
      data: () => ({ selected: "one", open: false, options: [{ value: "one", label: "一" }, { value: "two", label: "二" }] }),
      template: `<div><MenuItemRadio :selected="selected === 'one'">一</MenuItemRadio><Select v-model="selected" :options="options" aria-label="选择"/><Disclosure v-model="open"><template #summary>高级设置</template><span>内容</span></Disclosure><output data-testid="state">{{ selected }}|{{ open }}</output></div>`,
    });

    await expect.element(screen.getByRole("menuitemradio", { name: "一" })).toHaveAttribute("aria-checked", "true");
    await screen.getByRole("combobox", { name: "选择" }).selectOptions("two");
    await expect.element(screen.getByTestId("state")).toHaveTextContent("two|false");
    await screen.getByText("高级设置", { exact: true }).click();
    await expect.element(screen.getByTestId("state")).toHaveTextContent("two|true");
  });
});
