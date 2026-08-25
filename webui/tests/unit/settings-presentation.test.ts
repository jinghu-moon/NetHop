import { describe, expect, it } from "vitest";
import type { ConfigSchemaFieldDto } from "@/model/dto";
import { groupForSettingsField, isSettingsField, isUniqueEditorPath, settingsFieldLabel, settingsGroups, settingsSectionFields } from "@/model/settings-presentation";

function field(id: string, overrides: Partial<ConfigSchemaFieldDto> = {}): ConfigSchemaFieldDto {
  return {
    id,
    path: id,
    valueType: "boolean",
    title: `config.${id}.title`,
    group: id.split(".")[0] ?? "advanced",
    order: 1,
    advanced: false,
    experimental: false,
    sensitive: false,
    readOnly: false,
    applyImpact: "runtime_only",
    riskLevel: "normal",
    options: [],
    ...overrides,
  };
}

describe("settings presentation", () => {
  it("exposes only known daemon-backed fields", () => {
    expect(isSettingsField(field("network.proxy_tcp"))).toBe(true);
    expect(settingsFieldLabel(field("network.proxy_tcp"))).toBe("TCP 代理");
    expect(isSettingsField(field("network.unknown"))).toBe(false);
    expect(isSettingsField(field("network.wifi_scenes.rules", { sensitive: true }))).toBe(false);
  });

  it("keeps fields owned by other pages out of settings", () => {
    expect(isUniqueEditorPath("service.enabled")).toBe(true);
    expect(isSettingsField(field("service.enabled", { group: "service" }))).toBe(false);
    expect(isSettingsField(field("proxy.outbound_mode", { group: "proxy" }))).toBe(false);
  });

  it("groups fields with stable user-facing sections", () => {
    const groups = settingsGroups([field("network.proxy_tcp"), field("logging.level", { valueType: "enum", options: ["info"] })]);
    expect(groups.map((group) => group.title)).toEqual(["网络接管", "日志"]);
    expect(groupForSettingsField(field("network.proxy_tcp"))?.key).toBe("network");
  });

  it("keeps interface fields in their dedicated secondary editor", () => {
    const fields = [
      field("network.proxy_tcp"),
      field("network.interfaces.wifi"),
      field("network.wifi_scenes.probe_interval_seconds", { valueType: "integer" }),
    ];
    expect(settingsSectionFields(fields, "network").map((item) => item.path)).toEqual(["network.proxy_tcp"]);
    expect(settingsSectionFields(fields, "interfaces").map((item) => item.path)).toEqual([
      "network.interfaces.wifi",
      "network.wifi_scenes.probe_interval_seconds",
    ]);
  });
});
