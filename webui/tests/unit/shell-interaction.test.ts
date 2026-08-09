import { describe, expect, it } from "vitest";
import { keyboardVisible } from "@/shell/useKeyboardViewport";
import { dispatchBack, registerBackHandler } from "@/shell/useBackDispatcher";

describe("shell interaction boundaries", () => {
  it("detects only a material visual viewport reduction", () => {
    expect(keyboardVisible(800, 780)).toBe(false);
    expect(keyboardVisible(800, 640)).toBe(true);
  });

  it("closes the last registered surface first", () => {
    const calls: string[] = [];
    const first = registerBackHandler(() => { calls.push("first"); return true; });
    const second = registerBackHandler(() => { calls.push("second"); return true; });
    expect(dispatchBack()).toBe(true);
    expect(calls).toEqual(["second"]);
    second();
    expect(dispatchBack()).toBe(true);
    expect(calls).toEqual(["second", "first"]);
    first();
    expect(dispatchBack()).toBe(false);
  });
});
