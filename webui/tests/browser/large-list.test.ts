import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-vue";
import VirtualListViewport from "@/components/virtual/VirtualListViewport.vue";

describe("bounded virtual consumers", () => {
  it("keeps a 10,000 item node fixture bounded in the DOM", async () => {
    const items = Array.from({ length: 10_000 }, (_, index) => ({ id: `node-${index}`, name: `Node ${index}` }));
    const started = performance.now();
    render({ components: { VirtualListViewport }, data: () => ({ items }), template: `<VirtualListViewport :items="items" :get-item-key="(_index, item) => item.id" :estimate-size="56"><template #default="{ item }"><span>{{ item.name }}</span></template></VirtualListViewport>` });
    await expect.element(document.querySelector<HTMLElement>(".virtual-viewport")).toBeVisible();
    expect(performance.now() - started).toBeLessThan(100);
    expect(document.querySelectorAll(".virtual-row").length).toBeLessThan(40);
  });
});
