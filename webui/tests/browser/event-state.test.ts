import { render } from "vitest-browser-vue";
import { defineComponent, h, ref } from "vue";
import { describe, expect, it } from "vitest";

import { EventStateMachine } from "../../src/runtime/event-state";

describe("event state browser boundary", () => {
  it("renders stale metadata separately from daemon facts", async () => {
    const state = new EventStateMachine();
    state.apply({ version: 3, request_id: "events", sequence: 1, kind: "item", payload: { kind: "snapshot", runtime: { state: "running_tproxy" } } });
    state.markStale();
    const stale = ref(state.value().stale);
    const Probe = defineComponent({ setup: () => () => h("output", { class: "connection-state" }, stale.value ? "数据已过期" : "实时") });
    const screen = render(Probe);
    await expect.element(screen.getByText("数据已过期")).toBeVisible();
  });
});
