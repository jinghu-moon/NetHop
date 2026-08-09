import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-vue";

import StatusLine from "@/components/StatusLine.vue";
import MetricValue from "@/components/MetricValue.vue";
import OperationBanner from "@/components/OperationBanner.vue";
import PageState from "@/components/PageState.vue";
import SchemaField from "@/components/SchemaField.vue";
import TrafficSparkline from "@/components/TrafficSparkline.vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";

describe("shared WebUI components", () => {
  it("renders status, metrics and operation states", async () => {
    const screen = render({ components: { StatusLine, MetricValue, OperationBanner }, template: `<StatusLine status="degraded" label="网络降级"/><MetricValue label="下行" :value="1024" unit="B/s"/><OperationBanner phase="failure" message="更新失败"/>` });
    await expect.element(screen.getByText("网络降级")).toBeVisible();
    await expect.element(screen.getByText("1024")).toBeVisible();
    await expect.element(screen.getByText("更新失败")).toBeVisible();
    expect(document.querySelector(".t-notice-bar")).toBeNull();
  });

  it("renders bounded schema, empty state, canvas and virtual rows", async () => {
    const items = Array.from({ length: 100 }, (_, index) => ({ id: `item-${index}`, label: `Item ${index}` }));
    const screen = render({ components: { PageState, SchemaField, TrafficSparkline, VirtualListViewport }, data: () => ({ items }), template: `<PageState kind="empty" title="没有内容"/><SchemaField :field="{ id:'enabled', label:'启用', valueType:'bool', value:true }"/><TrafficSparkline :points="[{up:1,down:2,intervalSeconds:1,timestampMs:1},{up:2,down:4,intervalSeconds:1,timestampMs:2}]"/><VirtualListViewport :items="items" :get-item-key="(_i, item) => item.id"><template #default="{ item }"><span>{{ item.label }}</span></template></VirtualListViewport>` });
    await expect.element(screen.getByText("没有内容")).toBeVisible();
    await expect.element(document.querySelector<HTMLElement>(".t-empty")).toBeVisible();
    await expect.element(document.querySelector<HTMLCanvasElement>(".sparkline-wrap canvas")).toBeVisible();
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
