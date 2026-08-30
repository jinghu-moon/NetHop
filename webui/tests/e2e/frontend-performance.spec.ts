import { expect, test, type Page } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

type Direction = "enable" | "disable";

interface ToggleSample {
  readonly direction: Direction;
  readonly feedback_ms: number;
  readonly success_ms: number;
  readonly bridge_ms: number;
  readonly reported_ui_ms: number;
}

interface TimingStats {
  readonly count: number;
  readonly p50_ms: number;
  readonly p95_ms: number;
  readonly max_ms: number;
}

function percentile(values: readonly number[], percent: number): number {
  const ordered = [...values].sort((left, right) => left - right);
  if (ordered.length === 0) return 0;
  const rank = (ordered.length - 1) * percent / 100;
  const lower = Math.floor(rank);
  const upper = Math.ceil(rank);
  if (lower === upper) return ordered[lower]!;
  return ordered[lower]! + (ordered[upper]! - ordered[lower]!) * (rank - lower);
}

function stats(samples: readonly ToggleSample[], key: "feedback_ms" | "success_ms" | "bridge_ms" | "reported_ui_ms"): TimingStats {
  const values = samples.map((sample) => sample[key]);
  return {
    count: values.length,
    p50_ms: Math.round(percentile(values, 50) * 1000) / 1000,
    p95_ms: Math.round(percentile(values, 95) * 1000) / 1000,
    max_ms: Math.round(Math.max(...values) * 1000) / 1000,
  };
}

async function prepareTiming(page: Page): Promise<void> {
  await page.evaluate(() => {
    const state: { first_feedback_ms: number | null; started_at: number } = {
      first_feedback_ms: null,
      started_at: performance.now(),
    };
    const observer = new MutationObserver(() => {
      if (state.first_feedback_ms !== null) return;
      const message = document.querySelector<HTMLElement>(".operation-message");
      if (!message || message.dataset.phase === "idle") return;
      state.first_feedback_ms = performance.now() - state.started_at;
    });
    observer.observe(document.body, { attributes: true, childList: true, characterData: true, subtree: true });
    (window as Window & { __nethopPerf?: { state: typeof state; observer: MutationObserver } }).__nethopPerf = { state, observer };
  });
}

async function readTiming(page: Page): Promise<number> {
  return page.evaluate(() => {
    const perf = (window as Window & { __nethopPerf?: { state: { first_feedback_ms: number | null }; observer: MutationObserver } }).__nethopPerf;
    perf?.observer.disconnect();
    return perf?.state.first_feedback_ms ?? -1;
  });
}

function parseSuccessMessage(message: string): { bridge_ms: number; reported_ui_ms: number } {
  const match = message.match(/bridge\s+(\d+(?:\.\d+)?)\s+ms\s+·\s+UI\s+(\d+(?:\.\d+)?)\s+ms/);
  if (!match) throw new Error(`missing bridge/UI timing in operation message: ${message}`);
  return { bridge_ms: Number(match[1]), reported_ui_ms: Number(match[2]) };
}

test("capture toggle frontend timing stays within the browser budget", async ({ page }) => {
  const samplesPerDirection = Math.max(1, Number(process.env.NETHOP_FRONTEND_SAMPLES ?? 20));
  const switchControl = page.locator(".service-panel .nh-switch");
  const operationMessage = page.locator(".operation-message");
  const samples: ToggleSample[] = [];

  await page.setViewportSize({ width: 393, height: 873 });
  await page.addInitScript(() => {
    (window as Window & { __NETHOP_PERF_MOCK__?: boolean }).__NETHOP_PERF_MOCK__ = true;
  });
  await page.goto("/#/overview");
  await expect(switchControl).toBeVisible();

  for (let index = 0; index < samplesPerDirection; index += 1) {
    for (const direction of ["disable", "enable"] as const) {
      await prepareTiming(page);
      const started = performance.now();
      await switchControl.click();
      const expectedTitle = direction === "disable" ? "代理已关闭" : "代理已启动";
      await expect(operationMessage).toContainText(expectedTitle, { timeout: 2_000 });
      await expect(switchControl).toHaveAttribute("aria-checked", direction === "disable" ? "false" : "true");
      const successMs = performance.now() - started;
      const parsed = parseSuccessMessage(await operationMessage.innerText());
      const feedbackMs = await readTiming(page);
      expect(feedbackMs).toBeGreaterThanOrEqual(0);
      samples.push({
        direction,
        feedback_ms: Math.round(feedbackMs * 1000) / 1000,
        success_ms: Math.round(successMs * 1000) / 1000,
        bridge_ms: parsed.bridge_ms,
        reported_ui_ms: parsed.reported_ui_ms,
      });
    }
  }

  const byDirection = <K extends "feedback_ms" | "success_ms" | "bridge_ms" | "reported_ui_ms">(key: K) => ({
    enable: stats(samples.filter((sample) => sample.direction === "enable"), key),
    disable: stats(samples.filter((sample) => sample.direction === "disable"), key),
  });
  const summaries = {
    feedback: byDirection("feedback_ms"),
    success: byDirection("success_ms"),
    bridge: byDirection("bridge_ms"),
    reported_ui: byDirection("reported_ui_ms"),
  };

  const report = {
    schema: "nethop-webui-frontend-performance-v1",
    scope: "Chromium Pixel 7 emulation with the WebUI mock host; excludes Android WebView and real bridge transport",
    samples_per_direction: samplesPerDirection,
    budgets: { first_feedback_p95_ms: 100 },
    summaries,
    samples,
    passed: summaries.feedback.enable.p95_ms <= 100 && summaries.feedback.disable.p95_ms <= 100,
    contains_sensitive_data: false,
  };
  const outputRoot = path.resolve(process.cwd(), "..", "artifacts", "cli-performance", "webui");
  await mkdir(outputRoot, { recursive: true });
  const timestamp = new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
  await writeFile(path.join(outputRoot, `frontend-${timestamp}.json`), `${JSON.stringify(report, null, 2)}\n`, "utf8");
  expect(report.passed).toBe(true);
});
