import { describe, expect, it } from "vitest";

import {
  buildSourceSettings,
  parseSourceEditorSettings,
  type SourceEditorSettings,
} from "@/model/subscription-settings";

describe("subscription source settings", () => {
  it("reads bounded editable fields while keeping mirror values redacted", () => {
    const settings = parseSourceEditorSettings({
      subscriptions: {
        sources: [{
          source_id: "src_0123456789abcdef0123456789abcdef",
          request_profile: "mihomo",
          format_hint: "clash_yaml",
          mirrors: { configured_count: 2 },
          filter: {
            include_names: ["Premium"],
            exclude_names: ["Expired"],
            excluded_node_ids: ["nh1s-0123456789abcdef"],
            protocols: ["vless", "trojan"],
          },
        }],
      },
    }, "src_0123456789abcdef0123456789abcdef");

    expect(settings).toEqual({
      requestProfile: "mihomo",
      formatHint: "clash_yaml",
      mirrorCount: 2,
      mirrorsText: "",
      replaceMirrors: false,
      includeNamesText: "Premium",
      excludeNamesText: "Expired",
      protocols: ["vless", "trojan"],
    });
  });

  it("builds a full add value and a patch that preserves hidden mirrors", () => {
    const editor: SourceEditorSettings = {
      requestProfile: "sing_box_android",
      formatHint: "auto",
      mirrorCount: 1,
      mirrorsText: "",
      replaceMirrors: false,
      includeNamesText: "Premium\nPremium\nTokyo",
      excludeNamesText: "Expired",
      protocols: ["trojan", "vless"],
    };

    expect(buildSourceSettings(editor, "add")).toEqual({
      request_profile: "sing_box_android",
      format_hint: "auto",
      mirrors: [],
      filter: {
        include_names: ["Premium", "Tokyo"],
        exclude_names: ["Expired"],
        protocols: ["trojan", "vless"],
      },
    });
    expect(buildSourceSettings(editor, "update")).not.toHaveProperty("mirrors");
    expect(buildSourceSettings({ ...editor, replaceMirrors: true, mirrorsText: "https://one.example/sub\nhttps://two.example/sub" }, "update")).toMatchObject({
      mirrors: ["https://one.example/sub", "https://two.example/sub"],
    });
  });

  it("rejects unsupported protocols and oversized collections before upload", () => {
    const editor: SourceEditorSettings = {
      requestProfile: "generic",
      formatHint: "auto",
      mirrorCount: 0,
      mirrorsText: "",
      replaceMirrors: false,
      includeNamesText: "",
      excludeNamesText: "",
      protocols: ["wireguard"],
    };
    expect(() => buildSourceSettings(editor, "update")).toThrow("protocol");
    expect(() => buildSourceSettings({ ...editor, protocols: [], includeNamesText: Array.from({ length: 65 }, (_, index) => `Rule ${index}`).join("\n") }, "update")).toThrow("include");
  });
});
