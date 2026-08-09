import { describe, expect, it } from "vitest";

import { MINIMUM_WEBVIEW_MAJOR, isSupportedWebViewMajor } from "../../src/foundation/browserSupport";

describe("WebView build target", () => {
  it("accepts Chrome 105 and newer only", () => {
    expect(MINIMUM_WEBVIEW_MAJOR).toBe(105);
    expect(isSupportedWebViewMajor(104)).toBe(false);
    expect(isSupportedWebViewMajor(105)).toBe(true);
    expect(isSupportedWebViewMajor(130)).toBe(true);
    expect(isSupportedWebViewMajor(Number.NaN)).toBe(false);
  });
});
