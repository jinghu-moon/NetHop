import { expect, test, type Page } from "@playwright/test";

const routes = ["overview", "subscriptions", "nodes", "applications", "settings", "operations"] as const;
const viewports = [
  { name: "360x640", width: 360, height: 640 },
  { name: "393x873", width: 393, height: 873 },
  { name: "412x915", width: 412, height: 915 },
  { name: "600x960", width: 600, height: 960 },
] as const;

async function settle(page: Page): Promise<void> {
  await expect(page.locator("main h2")).toBeVisible();
  await page.waitForTimeout(180);
}

test("production routes remain offline", async ({ page }) => {
  const remoteRequests: string[] = [];
  page.on("request", (request) => {
    const url = request.url();
    if (!url.startsWith("http://127.0.0.1:4173/") && !url.startsWith("ksu://") && !url.startsWith("data:")) remoteRequests.push(url);
  });
  for (const route of routes) {
    await page.goto(`/#/${route}`);
    await settle(page);
  }
  expect(remoteRequests).toEqual([]);
});

test("secret input is cleared after the explicit edit lifecycle", async ({ page }) => {
  const canary = "https://canary.invalid/subscription?token=NH_SECRET_7f431";
  const consoleMessages: string[] = [];
  page.on("console", (message) => consoleMessages.push(message.text()));
  await page.goto("/#/subscriptions");
  await page.locator(".subscription-add-fab").click();
  await page.locator(".subscription-editor input").nth(1).fill(canary);
  await page.locator(".subscription-editor button").filter({ hasText: "取消" }).click();
  await expect(page.locator(".subscription-editor")).toHaveCount(0);
  const residue = await page.evaluate(() => ({
    html: document.documentElement.innerHTML,
    local: JSON.stringify(localStorage),
    session: JSON.stringify(sessionStorage),
  }));
  expect(JSON.stringify(residue)).not.toContain("NH_SECRET_7f431");
  expect(consoleMessages.join("\n")).not.toContain("NH_SECRET_7f431");
});

test("first interaction and lazy route transitions stay inside the desktop release budget", async ({ page }) => {
  const started = performance.now();
  await page.goto("/#/overview");
  await expect(page.locator(".service-panel .t-switch")).toBeVisible();
  expect(performance.now() - started).toBeLessThan(500);

  const samples: number[] = [];
  for (const route of ["subscriptions", "applications", "settings", "overview"] as const) {
    const before = performance.now();
    await page.goto(`/#/${route}`);
    await expect(page.locator("main h2")).toBeVisible();
    samples.push(performance.now() - before);
  }
  samples.sort((left, right) => left - right);
  expect(samples[Math.ceil(samples.length * 0.95) - 1]).toBeLessThan(100);
});

for (const viewport of viewports) {
  for (const theme of ["light", "dark"] as const) {
    test(`visual baseline ${viewport.name} ${theme}`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await page.emulateMedia({ colorScheme: theme });
      await page.goto("/#/overview");
      await settle(page);
      await expect(page).toHaveScreenshot(`overview-${viewport.name}-${theme}.png`, { fullPage: true });
    });
  }
}

test("operation feedback floats without shifting runtime controls", async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 873 });
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/#/overview");
  await settle(page);
  const serviceTop = await page.locator(".service-panel").evaluate((element) => element.getBoundingClientRect().top);
  await page.locator(".service-panel .t-switch").click();
  await expect(page.locator(".operation-message.t-message")).toContainText("代理已启动");
  expect(await page.locator(".service-panel").evaluate((element) => element.getBoundingClientRect().top)).toBe(serviceTop);
  await expect(page).toHaveScreenshot("overview-operation-success-393x873.png", { fullPage: true });
});

for (const theme of ["light", "dark"] as const) {
  test(`subscription cards visual baseline 393x873 ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width: 393, height: 873 });
    await page.emulateMedia({ colorScheme: theme });
    await page.goto("/#/subscriptions");
    await settle(page);
    await expect(page.locator(".source-card")).toHaveCount(2);
    await expect(page).toHaveScreenshot(`subscriptions-393x873-${theme}.png`, { fullPage: true });
  });
}

test("long text and error presentation remain bounded", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 640 });
  await page.goto("/#/overview");
  await settle(page);
  await page.locator(".overview-heading p").evaluate((element) => {
    element.textContent = "状态错误：这是用于验证极长诊断内容不会覆盖标题、主题控件或主导航的固定测试文本";
    element.classList.add("form-error");
  });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  await expect(page).toHaveScreenshot("overview-long-error-360x640.png", { fullPage: true });
});

test("icon and text content share the same visual center", async ({ page }) => {
  await page.goto("/#/overview");
  await settle(page);
  const offset = await page.locator(".proxy-quality-card .insight-heading").evaluate((element) => {
    const icon = element.querySelector(".insight-icon");
    const label = element.querySelector("strong");
    if (!icon || !label) return Number.POSITIVE_INFINITY;
    const iconRect = icon.getBoundingClientRect();
    const labelRect = label.getBoundingClientRect();
    return Math.abs((iconRect.top + iconRect.bottom) / 2 - (labelRect.top + labelRect.bottom) / 2);
  });
  expect(offset).toBeLessThanOrEqual(1);
});
