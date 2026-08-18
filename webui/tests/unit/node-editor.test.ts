import { describe, expect, it } from "vitest";

import {
  buildNodeOverridePayload,
  parseNodeOutbound,
  parseNodeOverride,
  serializeNodeOutbound,
} from "@/model/node-editor";

describe("node editor model", () => {
  it("parses a strict daemon-owned override snapshot", () => {
    const value = parseNodeOverride({
      node_id: "nh1s-0123456789abcdef",
      overridden: true,
      display_name: "东京节点",
      outbound: { type: "trojan", server: "edge.example.com", server_port: 443, password: "secret" },
    });
    expect(value.nodeId).toBe("nh1s-0123456789abcdef");
    expect(value.overridden).toBe(true);
    expect(serializeNodeOutbound(value.outbound)).toContain("edge.example.com");
  });

  it("rejects unknown fields and non-object outbound documents", () => {
    expect(() => parseNodeOverride({ node_id: "nh1s-0123456789abcdef", overridden: false, display_name: "node", outbound: {}, extra: true })).toThrow();
    expect(() => parseNodeOutbound("[]")).toThrow();
    expect(() => parseNodeOutbound(JSON.stringify({ type: "trojan" }))).toThrow();
  });

  it("builds the exact typed private payload without flattening credentials", () => {
    const payload = JSON.parse(buildNodeOverridePayload(
      "nh1s-0123456789abcdef",
      "东京节点",
      parseNodeOutbound(JSON.stringify({ type: "trojan", server: "edge.example.com", server_port: 443, password: "secret" })),
    ));
    expect(payload.target).toBe("nh1s-0123456789abcdef");
    expect(payload.node_override.display_name).toBe("东京节点");
    expect(payload.node_override.outbound.password).toBe("secret");
  });
});
