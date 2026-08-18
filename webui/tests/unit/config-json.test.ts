import { describe, expect, it } from "vitest";

import { parseConfigDocument, serializeConfigDocument } from "@/model/config-json";

describe("expert JSON config editor", () => {
  it("round-trips one bounded object without changing values", () => {
    const document = { schema_version: 3, service: { enabled: true }, subscriptions: { sources: [] } };
    expect(parseConfigDocument(serializeConfigDocument(document))).toEqual(document);
  });

  it("rejects arrays, primitives, duplicate keys and oversized input", () => {
    for (const input of ["[]", "null", "true", '{"service":{},"service":{}}', `{"value":"${"x".repeat(1024 * 1024)}"}`]) {
      expect(() => parseConfigDocument(input)).toThrow();
    }
  });
});
