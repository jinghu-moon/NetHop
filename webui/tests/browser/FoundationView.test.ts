import { render } from "vitest-browser-vue";
import { describe, expect, it } from "vitest";

import FoundationView from "../../src/views/FoundationView.vue";

describe("foundation view", () => {
  it("renders the local control shell with a reconnect command", async () => {
    render(FoundationView);
    await expect.element(document.querySelector<HTMLElement>(".foundation-header h1")).toHaveTextContent("NetHop");
    await expect.element(document.querySelector<HTMLElement>(".foundation-header .t-button")).toBeVisible();
  });
});
