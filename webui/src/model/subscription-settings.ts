export const subscriptionRequestProfiles = ["generic", "mihomo", "clash_standard", "surfboard", "sing_box", "sing_box_android"] as const;
export const subscriptionFormatHints = ["auto", "uri_list", "base64_list", "clash_yaml", "singbox_json", "surfboard_ini"] as const;
export const subscriptionProtocols = ["vless", "vmess", "shadowsocks", "trojan", "hysteria2", "tuic", "anytls", "http", "socks"] as const;

export type SubscriptionRequestProfile = typeof subscriptionRequestProfiles[number];
export type SubscriptionFormatHint = typeof subscriptionFormatHints[number];

export interface SourceEditorSettings {
  requestProfile: SubscriptionRequestProfile;
  formatHint: SubscriptionFormatHint;
  mirrorCount: number;
  mirrorsText: string;
  replaceMirrors: boolean;
  includeNamesText: string;
  excludeNamesText: string;
  protocols: string[];
}

export const defaultSourceEditorSettings: SourceEditorSettings = {
  requestProfile: "sing_box_android",
  formatHint: "auto",
  mirrorCount: 0,
  mirrorsText: "",
  replaceMirrors: false,
  includeNamesText: "",
  excludeNamesText: "",
  protocols: [],
};

export function parseSourceEditorSettings(document: Readonly<Record<string, unknown>>, sourceId: string): SourceEditorSettings {
  const subscriptions = object(document.subscriptions);
  const sources = Array.isArray(subscriptions.sources) ? subscriptions.sources : [];
  const source = sources.map(object).find((item) => item.source_id === sourceId);
  if (!source) throw new Error("subscription source is missing");
  const filter = object(source.filter);
  const mirrors = object(source.mirrors);
  return {
    requestProfile: member(source.request_profile, subscriptionRequestProfiles, "request profile", defaultSourceEditorSettings.requestProfile),
    formatHint: member(source.format_hint, subscriptionFormatHints, "format hint", defaultSourceEditorSettings.formatHint),
    mirrorCount: integer(mirrors.configured_count, 0, 4, 0),
    mirrorsText: "",
    replaceMirrors: false,
    includeNamesText: stringList(filter.include_names, "include").join("\n"),
    excludeNamesText: stringList(filter.exclude_names, "exclude").join("\n"),
    protocols: stringList(filter.protocols, "protocol"),
  };
}

export function buildSourceSettings(settings: SourceEditorSettings, mode: "add" | "update"): Readonly<Record<string, unknown>> {
  const includeNames = lines(settings.includeNamesText, "include", 64, 128);
  const excludeNames = lines(settings.excludeNamesText, "exclude", 64, 128);
  const protocols = unique(settings.protocols);
  if (protocols.length > 16 || protocols.some((value) => !subscriptionProtocols.includes(value as typeof subscriptionProtocols[number]))) {
    throw new Error("invalid protocol filter");
  }
  const mirrors = lines(settings.mirrorsText, "mirror", 4, 16 * 1024);
  if (mirrors.some((value) => !/^https:\/\//i.test(value))) throw new Error("mirror must use HTTPS");
  return {
    request_profile: settings.requestProfile,
    format_hint: settings.formatHint,
    ...(mode === "add" || settings.replaceMirrors ? { mirrors } : {}),
    filter: {
      include_names: includeNames,
      exclude_names: excludeNames,
      protocols,
    },
  };
}

function object(value: unknown): Readonly<Record<string, unknown>> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? value as Readonly<Record<string, unknown>>
    : {};
}

function member<T extends string>(value: unknown, values: readonly T[], label: string, fallback: T): T {
  if (value === undefined) return fallback;
  if (typeof value !== "string" || !values.includes(value as T)) throw new Error(`invalid ${label}`);
  return value as T;
}

function integer(value: unknown, min: number, max: number, fallback: number): number {
  if (value === undefined) return fallback;
  if (!Number.isInteger(value) || (value as number) < min || (value as number) > max) throw new Error("invalid integer");
  return value as number;
}

function stringList(value: unknown, label: string): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) throw new Error(`invalid ${label} list`);
  return unique(value as string[]);
}

function lines(value: string, label: string, limit: number, maxBytes: number): string[] {
  const values = unique(value.split(/\r?\n/).map((item) => item.trim()).filter(Boolean));
  if (values.length > limit || values.some((item) => item.length > maxBytes || /[\u0000-\u001f\u007f]/.test(item))) {
    throw new Error(`invalid ${label} list`);
  }
  return values;
}

function unique(values: readonly string[]): string[] {
  return [...new Set(values)];
}
