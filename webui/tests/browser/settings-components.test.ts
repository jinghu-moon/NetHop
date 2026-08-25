import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-vue";

import SettingsFieldControl from "@/components/settings/SettingsFieldControl.vue";
import SettingsGroup from "@/components/settings/SettingsGroup.vue";
import SettingsRow from "@/components/settings/SettingsRow.vue";

describe("custom settings components", () => {
  it("updates boolean, integer and enum values without TDesign controls", async () => {
    const screen = render({
      components: { SettingsFieldControl },
      data: () => ({ enabled: true, count: 2, mode: "auto" }),
      template: `
        <div>
          <SettingsFieldControl :field="{ label: '启用代理', valueType: 'bool', value: enabled }" @change="enabled = $event" />
          <SettingsFieldControl :field="{ label: '候选数', valueType: 'int', value: count, minimum: 1, maximum: 3 }" @change="count = $event" />
          <SettingsFieldControl :field="{ label: 'DNS 模式', valueType: 'enum', value: mode, options: ['auto', 'proxy'] }" @change="mode = $event" />
          <output data-testid="state">{{ enabled }}|{{ count }}|{{ mode }}</output>
        </div>
      `,
    });

    await screen.getByRole("switch", { name: "启用代理" }).click();
    await screen.getByRole("button", { name: "增加" }).click();
    await screen.getByRole("combobox", { name: "DNS 模式" }).click();
    await screen.getByRole("option", { name: "proxy" }).click();
    await expect.element(screen.getByTestId("state")).toHaveTextContent("false|3|proxy");
  });

  it("keeps unsupported controls disabled and exposes the reason", async () => {
    const screen = render({
      components: { SettingsFieldControl },
      template: `<SettingsFieldControl :field="{ label: '热点代理', valueType: 'bool', value: false, disabledReason: 'unsupported: hotspot_not_available' }" />`,
    });

    await expect.element(screen.getByRole("switch", { name: "热点代理" })).toBeDisabled();
    await expect.element(screen.getByText("unsupported: hotspot_not_available")).toBeVisible();
  });

  it("composes grouped navigation rows with a semantic button", async () => {
    const screen = render({
      components: { SettingsGroup, SettingsRow },
      data: () => ({ activated: false }),
      template: `<SettingsGroup title="网络"><SettingsRow title="网络接管" description="DNS 与 IPv6" clickable arrow @activate="activated = true" /></SettingsGroup><output data-testid="activated">{{ activated }}</output>`,
    });

    await screen.getByRole("button", { name: /网络接管/ }).click();
    await expect.element(screen.getByTestId("activated")).toHaveTextContent("true");
  });
});
