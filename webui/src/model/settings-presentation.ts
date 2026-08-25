import type { ConfigSchemaFieldDto } from "./dto";

export interface SettingsGroupDefinition {
  readonly key: string;
  readonly title: string;
  readonly description: string;
  readonly order: number;
}

export type SettingsSectionKey = "updates" | "network" | "interfaces" | "routing" | "logging" | "advanced";

export interface SettingsSectionDefinition {
  readonly key: SettingsSectionKey;
  readonly title: string;
  readonly description: string;
  readonly group: string;
}

export const SETTINGS_SECTIONS: readonly SettingsSectionDefinition[] = [
  { key: "updates", title: "更新与自动选择", description: "订阅调度和节点自动选择参数", group: "proxy" },
  { key: "network", title: "网络接管", description: "协议、DNS、IPv6 和 TUN 栈", group: "network" },
  { key: "interfaces", title: "接口范围", description: "移动网络、Wi-Fi、热点和 USB 网络共享", group: "network" },
  { key: "routing", title: "路由策略", description: "私网、中国大陆和 QUIC 处理策略", group: "routing" },
  { key: "logging", title: "日志", description: "日志等级和保留期限", group: "logging" },
  { key: "advanced", title: "高级设置", description: "端口、健康检查和实验参数", group: "advanced" },
] as const;

export const SETTINGS_GROUPS: readonly SettingsGroupDefinition[] = [
  { key: "proxy", title: "更新与自动选择", description: "订阅调度和节点自动选择参数", order: 10 },
  { key: "network", title: "网络接管", description: "协议、DNS、IPv6 和接口范围", order: 20 },
  { key: "routing", title: "路由策略", description: "私网、中国大陆和 QUIC 处理策略", order: 30 },
  { key: "logging", title: "日志", description: "日志等级和保留期限", order: 40 },
  { key: "advanced", title: "高级设置", description: "端口、健康检查和实验参数", order: 50 },
] as const;

const FIELD_LABELS: Readonly<Record<string, string>> = {
  "subscriptions.auto_update": "订阅自动更新",
  "subscriptions.update_interval_hours": "订阅更新间隔",
  "proxy.urltest.interval_minutes": "自动测速间隔",
  "proxy.urltest.tolerance_ms": "节点切换容差",
  "proxy.urltest.max_candidates": "最大自动候选数",
  "network.proxy_tcp": "TCP 代理",
  "network.proxy_udp": "UDP 代理",
  "network.ipv6_mode": "IPv6 策略",
  "network.dns_mode": "DNS 模式",
  "network.interfaces.mobile": "移动网络",
  "network.interfaces.wifi": "Wi-Fi",
  "network.interfaces.hotspot": "热点代理",
  "network.interfaces.usb": "USB 网络共享",
  "network.wifi_scenes.enabled": "Wi-Fi 场景规则",
  "network.wifi_scenes.probe_interval_seconds": "Wi-Fi 场景探测间隔",
  "network.tun_stack": "TUN 栈",
  "routing.bypass_private": "绕过私有网络",
  "routing.bypass_cn": "绕过中国大陆规则",
  "routing.block_quic": "阻断 QUIC",
  "logging.level": "日志级别",
  "logging.retention_days": "日志保留天数",
  "advanced.inbound_port": "入站端口",
  "advanced.bypass_mark": "bypass mark",
  "advanced.ipv6_guard": "IPv6 guard",
  "advanced.dry_run": "仅验证不激活",
  "advanced.health_timeout_seconds": "健康检查超时",
  "advanced.reconcile_interval_seconds": "协调间隔",
};

const HIDDEN_PATHS = new Set([
  "service.enabled",
  "proxy.outbound_mode",
  "network.capture_mode",
  "subscriptions.mode",
  "subscriptions.sources",
  "applications.mode",
  "applications.targets",
]);

export function settingsFieldLabel(field: ConfigSchemaFieldDto): string | undefined {
  return FIELD_LABELS[field.id];
}

export function isSettingsField(field: ConfigSchemaFieldDto): boolean {
  return !HIDDEN_PATHS.has(field.path)
    && !field.sensitive
    && !field.readOnly
    && !field.path.includes("[]")
    && settingsFieldLabel(field) !== undefined;
}

export function groupForSettingsField(field: ConfigSchemaFieldDto): SettingsGroupDefinition | undefined {
  if (!isSettingsField(field)) return undefined;
  const groupKey = field.group === "subscriptions" ? "proxy" : field.group;
  return SETTINGS_GROUPS.find((group) => group.key === groupKey);
}

export function settingsGroups(fields: readonly ConfigSchemaFieldDto[]): readonly (SettingsGroupDefinition & { readonly fields: readonly ConfigSchemaFieldDto[] })[] {
  return SETTINGS_GROUPS.map((group) => ({
    ...group,
    fields: fields
      .filter((field) => groupForSettingsField(field)?.key === group.key)
      .sort((left, right) => left.order - right.order),
  })).filter((group) => group.fields.length > 0);
}

export function settingsSection(key: SettingsSectionKey): SettingsSectionDefinition {
  return SETTINGS_SECTIONS.find((section) => section.key === key) ?? SETTINGS_SECTIONS[0]!;
}

export function settingsSectionFields(fields: readonly ConfigSchemaFieldDto[], key: SettingsSectionKey): readonly ConfigSchemaFieldDto[] {
  const section = settingsSection(key);
  return fields
    .filter((field) => groupForSettingsField(field)?.key === section.group)
    .filter((field) => key !== "interfaces" || field.path.startsWith("network.interfaces.") || field.path.startsWith("network.wifi_scenes."))
    .filter((field) => key !== "network" || (!field.path.startsWith("network.interfaces.") && !field.path.startsWith("network.wifi_scenes.")))
    .sort((left, right) => left.order - right.order);
}

export function isUniqueEditorPath(path: string): boolean {
  return HIDDEN_PATHS.has(path);
}
