import { expect, test } from "@playwright/test";

test("production WebUI loads from a hash route without remote requests", async ({ page }) => {
  const externalRequests: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (url.hostname !== "127.0.0.1") externalRequests.push(request.url());
  });
  await page.goto("/#/");
  await expect(page.locator("main h2").filter({ hasText: "概览" })).toBeVisible();
  await expect(page.locator(".app-header")).toHaveCount(0);
  await expect(page.locator(".foundation-header .t-button")).toHaveCount(0);
  await expect(page.locator(".t-tab-bar")).toBeVisible();
  for (const [route, label] of [["overview", "概览"], ["subscriptions", "订阅"], ["nodes", "节点"], ["applications", "应用"], ["settings", "设置"]] as const) {
    await page.goto(`/#/${route}`);
    await expect(page.locator("main h2").filter({ hasText: label })).toBeVisible();
    await expect(page.locator("select")).toHaveCount(0);
  }
  expect(externalRequests).toEqual([]);
  expect(await page.locator('meta[http-equiv="Content-Security-Policy"]').count()).toBe(1);
});

test("daily controls and subscription forms remain typed and mobile-safe", async ({ page }) => {
  await page.goto("/#/overview");
  await expect(page.locator(".service-panel .t-switch")).toBeVisible();
  await expect(page.locator(".proxy-quality-card")).toHaveAttribute("href", "#/nodes");
  await page.goto("/#/subscriptions");
  const addSubscription = page.locator(".subscription-add-fab");
  await expect(page.locator(".subscriptions-page .heading-actions .t-button")).toHaveCount(1);
  await expect(addSubscription).toHaveCSS("position", "fixed");
  await expect(addSubscription.locator(".tabler-icon-plus")).toBeVisible();
  await expect(addSubscription).toHaveText("");
  const addBox = await addSubscription.boundingBox();
  const subscriptionTabBar = await page.locator(".t-tab-bar").boundingBox();
  expect(addBox).not.toBeNull();
  expect(subscriptionTabBar).not.toBeNull();
  expect(addBox!.width).toBe(48);
  expect(addBox!.height).toBe(48);
  expect(addBox!.x + addBox!.width).toBeLessThanOrEqual(page.viewportSize()!.width - 15);
  expect(addBox!.y + addBox!.height).toBeLessThanOrEqual(subscriptionTabBar!.y - 15);
  await addSubscription.click();
  await expect(page.getByText("添加订阅", { exact: true }).last()).toBeVisible();
  await expect(page.locator(".subscription-editor .t-input").nth(0)).toBeVisible();
  await expect(page.locator(".subscription-editor .t-input").nth(1)).toBeVisible();
  await expect(page.locator(".subscription-editor").getByText(/^ID$/i)).toHaveCount(0);
  await page.getByText("从文本内容导入", { exact: true }).click();
  await expect(page.locator("button").filter({ hasText: "预览" })).toBeDisabled();
  await expect(page.locator("button").filter({ hasText: "确认导入" })).toBeDisabled();
});

test("subscription cards expose real daemon status and move secondary actions into TDesign ActionSheet", async ({ page }) => {
  await page.goto("/#/subscriptions");
  await expect(page.locator(".source-card")).toHaveCount(2);
  await expect(page.locator(".source-selector.t-radio")).toHaveCount(2);
  for (const card of await page.locator(".source-card").all()) {
    const geometry = await card.evaluate((element) => {
      const selector = element.querySelector<HTMLElement>(".source-selector");
      const icon = element.querySelector<HTMLElement>(".source-selector .t-radio__icon");
      const main = element.querySelector<HTMLElement>(".source-main");
      if (!selector || !icon || !main) return { inset: -1, gap: -1, iconStart: -1, iconEnd: -1, block: true };
      const cardBox = element.getBoundingClientRect();
      const selectorBox = selector.getBoundingClientRect();
      const iconBox = icon.getBoundingClientRect();
      const mainBox = main.getBoundingClientRect();
      return { inset: selectorBox.left - cardBox.left, gap: mainBox.left - selectorBox.right, iconStart: iconBox.left - selectorBox.left, iconEnd: selectorBox.right - iconBox.right, block: selector.classList.contains("t-radio--block") };
    });
    expect(geometry.inset).toBe(13);
    expect(geometry.gap).toBe(10);
    expect(geometry.iconStart).toBe(0);
    expect(geometry.iconEnd).toBe(0);
    expect(geometry.block).toBe(false);
  }
  await expect(page.locator(".source-selector .t-radio__border")).toHaveCount(0);
  await expect(page.locator(".source-card").first()).toHaveAttribute("data-selected", "true");
  await page.locator(".source-selector.t-radio").nth(1).click();
  await expect(page.locator(".source-card").nth(1)).toHaveAttribute("data-selected", "true");
  await expect(page.locator(".source-card").first()).toHaveAttribute("data-selected", "false");
  await expect(page.locator(".source-card").first()).toContainText("128.4 / 200 GB");
  await expect(page.locator(".source-card").first()).toContainText("46 节点");
  await expect(page.locator(".source-card").first().locator(".source-quota-track i")).toHaveCount(1);
  await expect(page.locator(".source-card").nth(1)).toContainText("-- / -- GB · 剩余 -- 天 · -- 节点");
  await expect(page.locator(".source-card").nth(1).locator(".source-quota-track[data-empty='true']")).toHaveCount(1);
  await page.locator(".source-card").first().locator(".source-actions .t-button").last().click();
  const sheet = page.locator(".subscription-actions-sheet");
  await expect(sheet).toBeVisible();
  await expect(sheet).toContainText("更新订阅");
  await expect(sheet).toContainText("编辑");
  await expect(sheet).toContainText("删除");
  await expect(sheet).not.toContainText("复制订阅链接");
  await sheet.getByText("编辑", { exact: true }).click();
  await expect(page.locator(".subscription-editor")).toBeVisible();
  await expect(page.locator(".subscription-editor")).toContainText("编辑订阅");
});

test("overview presents the compact runtime control hierarchy", async ({ page }) => {
  await page.goto("/#/overview");
  const servicePanel = page.locator(".service-panel");
  await expect(servicePanel.locator(".t-switch")).toBeVisible();
  await expect(page.locator(".traffic-section .traffic-rate")).toHaveCount(2);

  const mode = page.locator(".overview-mode:not(.capture-mode) .segmented-control");
  await expect(mode).toBeVisible();
  await expect(mode).toContainText("规则");
  await expect(mode).toContainText("全局");
  await expect(mode).toContainText("直连");

  const capture = page.locator(".capture-mode .segmented-control");
  await expect(capture).toBeVisible();
  await expect(capture).toContainText("自动");
  await expect(capture).toContainText("TPROXY");
  await expect(capture).toContainText("TUN");

  await expect(page.locator(".node-summary")).toBeVisible();
  await expect(page.locator(".node-summary")).toHaveAttribute("href", "#/nodes");
  await expect(page.locator(".runtime-card")).toBeVisible();
  await expect(page.locator(".subscription-link")).toHaveCount(0);
  const overviewCardSections = page.locator(".service-control, .overview-mode, .traffic-section, .overview-insight-card");
  for (let index = 0; index < await overviewCardSections.count(); index += 1) {
    expect(await overviewCardSections.nth(index).evaluate((element) => {
      const style = getComputedStyle(element);
      return [style.paddingTop, style.paddingRight, style.paddingBottom, style.paddingLeft];
    })).toEqual(["12px", "12px", "12px", "12px"]);
  }
  const insightGrid = page.locator(".overview-insight-grid");
  await insightGrid.scrollIntoViewIfNeeded();
  const insightBox = await insightGrid.boundingBox();
  const tabBarBox = await page.locator(".t-tab-bar").boundingBox();
  expect(insightBox).not.toBeNull();
  expect(tabBarBox).not.toBeNull();
  expect(insightBox!.y + insightBox!.height).toBeLessThanOrEqual(tabBarBox!.y);
  expect(await page.locator(".traffic-section .sparkline-wrap").evaluate((element) => element.getBoundingClientRect().height)).toBeGreaterThanOrEqual(70);
  await expect(page.locator(".summary-grid")).toHaveCount(0);
  await expect(page.getByText("Generation", { exact: true })).toHaveCount(0);
  await page.locator(".node-summary").click();
  await expect(page).toHaveURL(/#\/nodes$/);
});

test("secondary node, settings and operations routes load without server routing", async ({ page }) => {
  for (const [route, heading] of [["nodes", "节点"], ["applications", "应用"], ["settings", "设置"], ["operations", "运维"]] as const) {
    await page.goto(`/#/${route}`);
    await expect(page.locator("main h2").filter({ hasText: heading })).toBeVisible();
  }
  await page.goto("/#/operations");
  await page.locator("button").filter({ hasText: "关闭全部" }).click();
  await expect(page.getByText("所有当前代理连接都将中断，是否继续？")).toBeVisible();
  await page.goto("/#/settings");
  await expect(page.locator("button").filter({ hasText: "验证" })).toBeDisabled();
});

test("node page tests every node with one action and renders two equal columns", async ({ page }) => {
  await page.setViewportSize({ width: 393, height: 852 });
  await page.goto("/#/nodes");
  await expect(page.locator(".t-pull-down-refresh")).toHaveCount(1);
  const testAll = page.getByTitle("测试全部节点");
  await expect(testAll).toHaveCount(1);
  await expect(page.locator(".node-card .tabler-icon-activity")).toHaveCount(0);
  const firstRow = page.locator(".node-grid-row").first();
  await expect(firstRow.locator(".node-card")).toHaveCount(2);
  const geometry = await firstRow.evaluate((element) => {
    const cards = [...element.querySelectorAll<HTMLElement>(".node-card")].map((card) => card.getBoundingClientRect());
    return { widths: cards.map((card) => card.width), gap: cards[1]!.left - cards[0]!.right };
  });
  expect(Math.abs(geometry.widths[0]! - geometry.widths[1]!)).toBeLessThanOrEqual(0.5);
  expect(geometry.gap).toBe(8);
  await testAll.click();
  await expect(page.locator(".node-card").first()).toContainText("64 ms");
  await expect(page.locator(".operation-message")).toContainText("测速完成：成功 4 / 4");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
});

test("P0-P1 node, application, overlay, cache, log and metrics flows are integrated", async ({ page }) => {
  await page.goto("/#/nodes");
  await expect(page.locator(".node-source-heading")).toHaveCount(2);
  await expect(page.locator(".node-source-heading").nth(0)).toContainText("Primary");
  await expect(page.locator(".node-source-heading").nth(1)).toContainText("Backup");
  await page.getByTitle("更多操作").click();
  await expect(page.getByText("按延迟排序", { exact: true })).toBeVisible();
  await page.evaluate(() => window.dispatchEvent(new Event("nethop:back")));
  await expect(page.locator(".node-actions-sheet")).not.toBeVisible();
  await page.getByTitle("更多操作").click();
  await page.getByText("按延迟排序", { exact: true }).click();
  await expect(page.locator(".node-grid-row").nth(1).locator(".node-card").first()).toContainText("东京 · 低延迟");

  await page.goto("/#/applications");
  await expect(page.locator(".t-pull-down-refresh")).toHaveCount(1);
  await page.getByTitle("排序方式").click();
  await page.getByText("存储占用 · 降序", { exact: true }).click();
  await expect(page.locator(".app-row").first()).toContainText("YouTube");
  await page.getByTitle("排序方式").click();
  await page.getByText("已选优先", { exact: true }).click();
  await expect(page.locator(".app-row").first()).toContainText("哔哩哔哩");
  await page.locator(".application-search input").fill("YouTube");
  await page.goto("/#/settings");
  await page.goto("/#/applications");
  await expect(page.locator(".application-search input")).toHaveValue("YouTube");

  await page.goto("/#/operations");
  await page.getByText("日志", { exact: true }).click();
  const logControls = page.locator(".log-controls");
  await expect(logControls.getByText("服务", { exact: true })).toBeVisible();
  await expect(logControls.getByText("订阅", { exact: true })).toBeVisible();
  await expect(logControls.getByText("内核", { exact: true })).toBeVisible();
  await expect(page.locator(".log-row")).toContainText("daemon ready");
  await logControls.getByText("原始", { exact: true }).click();
  await expect(page.locator(".raw-log-row")).toContainText("service_ready");
  await page.getByText("系统", { exact: true }).click();
  await expect(page.locator(".operation-grid")).toContainText("核心 CPU");
  await expect(page.locator(".operation-grid")).toContainText("32 MiB");
  await expect(page.locator(".operation-grid")).toContainText("wlan0");
});

test("TDesign dropdowns replace native selects and commit the selected option", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/#/settings");
  await expect(page.locator("html")).toHaveAttribute("theme-mode", "dark");
  await page.locator(".theme-dropdown .t-dropdown-menu__item").click();
  await page.getByText("深色", { exact: true }).last().click();
  await expect(page.locator("html")).toHaveAttribute("data-theme-mode", "dark");
  await expect(page.locator(".settings-utilities .t-button")).toBeVisible();
  await expect(page.locator("select")).toHaveCount(0);
});

test("YingLi semantic color system drives NetHop and TDesign in both themes", async ({ page }) => {
  const readTokens = () => page.locator("html").evaluate((element) => {
    const probe = document.createElement("span");
    element.append(probe);
    const tokens = Object.fromEntries([
      "--nh-bg",
      "--nh-surface",
      "--nh-text",
      "--nh-muted",
      "--nh-info",
      "--nh-selection",
      "--td-brand-color",
      "--td-button-primary-color",
      "--td-bg-color-page",
    ].map((name) => {
      probe.style.color = `var(${name})`;
      return [name, getComputedStyle(probe).color];
    }));
    probe.remove();
    return tokens;
  });

  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/#/overview");
  expect(await readTokens()).toEqual({
    "--nh-bg": "rgb(245, 245, 245)",
    "--nh-surface": "rgb(255, 255, 255)",
    "--nh-text": "rgb(31, 31, 31)",
    "--nh-muted": "rgb(82, 82, 82)",
    "--nh-info": "rgb(71, 123, 148)",
    "--nh-selection": "rgb(31, 31, 31)",
    "--td-brand-color": "rgb(31, 31, 31)",
    "--td-button-primary-color": "rgb(255, 255, 255)",
    "--td-bg-color-page": "rgb(245, 245, 245)",
  });

  await page.emulateMedia({ colorScheme: "dark" });
  await page.reload();
  expect(await readTokens()).toEqual({
    "--nh-bg": "rgb(15, 15, 15)",
    "--nh-surface": "rgb(23, 23, 23)",
    "--nh-text": "rgb(245, 245, 245)",
    "--nh-muted": "rgb(184, 184, 184)",
    "--nh-info": "rgb(152, 195, 213)",
    "--nh-selection": "rgb(245, 245, 245)",
    "--td-brand-color": "rgb(245, 245, 245)",
    "--td-button-primary-color": "rgb(18, 18, 18)",
    "--td-bg-color-page": "rgb(15, 15, 15)",
  });
});

test("active bottom tab uses the structural selection pill in both themes", async ({ page }) => {
  const activeIcon = page.locator(".t-tab-bar-item__content--checked .t-tab-bar-item__icon");

  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/#/overview");
  await expect(activeIcon).toHaveCSS("background-color", "rgb(31, 31, 31)");
  await expect(activeIcon).toHaveCSS("color", "rgb(255, 255, 255)");
  await expect(activeIcon).toHaveCSS("width", "42px");
  await expect(activeIcon).toHaveCSS("height", "28px");

  await page.emulateMedia({ colorScheme: "dark" });
  await page.reload();
  await expect(activeIcon).toHaveCSS("background-color", "rgb(245, 245, 245)");
  await expect(activeIcon).toHaveCSS("color", "rgb(18, 18, 18)");
});

test("application policy uses the shared animated segmented control and mode-specific content", async ({ page }) => {
  await page.goto("/#/applications");
  const segmented = page.locator(".application-mode .segmented-control");
  await expect(segmented).toBeVisible();
  await expect(segmented.locator('input[type="radio"]')).toHaveCount(0);
  await expect(segmented.locator('.segmented-item[data-active="true"]')).toHaveText("黑名单");
  await expect(page.locator(".application-mode-hint")).toHaveCount(0);
  await expect(page.locator(".application-save")).toHaveCount(0);
  await expect(page.locator(".application-selection-summary")).toContainText("已选应用");
  await expect(page.locator(".application-selected-count")).toHaveText("1");
  await expect(page.locator(".application-selection-effect")).toContainText("选中应用直连");
  await expect(page.locator(".application-search input")).toBeVisible();
  await expect(page.locator(".filter-bar .t-search")).toHaveCount(0);
  const categoryDropdown = page.locator(".application-category-dropdown");
  await expect(categoryDropdown).toContainText("用户应用");
  await categoryDropdown.locator(".application-category-trigger").click();
  await expect(categoryDropdown.locator(".application-category-menu")).toBeVisible();
  await categoryDropdown.getByText("系统应用", { exact: true }).click();
  await expect(page.locator(".app-row .t-switch")).toHaveCount(1);
  await categoryDropdown.locator(".application-category-trigger").click();
  await categoryDropdown.getByText("用户应用", { exact: true }).click();
  await expect(page.locator(".app-row .t-switch")).toHaveCount(2);
  await expect(page.locator(".app-row").first()).toHaveAttribute("data-selected", "true");
  await expect(page.locator(".app-row").nth(1)).toHaveAttribute("data-selected", "false");
  expect(await page.locator(".application-list .virtual-viewport").evaluate((element) => element.getBoundingClientRect().height)).toBeLessThanOrEqual(157);
  await expect(page.getByText("应用模式", { exact: true })).toHaveCount(0);
  await expect(page.getByText("保存选择", { exact: true })).toHaveCount(0);
  const thumbTransition = await segmented.locator(".segmented-indicator").evaluate((element) => getComputedStyle(element).transition);
  expect(thumbTransition).toContain("0.35s");
  await segmented.getByText("全部应用", { exact: true }).click();
  await expect(segmented.locator('.segmented-item[data-active="true"]')).toHaveText("全部应用");
  await expect(page.locator(".application-selection-summary")).toHaveCount(0);
  await expect(page.locator(".filter-bar")).toHaveCount(0);
  await segmented.getByText("白名单", { exact: true }).click();
  await expect(segmented.locator('.segmented-item[data-active="true"]')).toHaveText("白名单");
  await expect(page.locator(".application-selection-summary")).toContainText("仅选中应用代理");
  await expect(page.locator(".operation-message")).toContainText("应用策略已自动保存", { timeout: 2_000 });
});

test("application page sorts by persisted metadata preferences from the more menu", async ({ page }) => {
  await page.goto("/#/applications");
  const sortButton = page.getByTitle("排序方式");
  await expect(sortButton).toBeVisible();
  await sortButton.click();

  const sheet = page.locator(".application-sort-sheet");
  await expect(sheet).toBeVisible();
  for (const label of ["名称 · 升序", "名称 · 降序", "更新时间 · 升序", "更新时间 · 降序", "存储占用 · 升序", "存储占用 · 降序", "最近使用时间 · 升序", "最近使用时间 · 降序"]) {
    await expect(sheet.getByText(label, { exact: true })).toBeVisible();
  }

  await sheet.getByText("存储占用 · 降序", { exact: true }).click();
  await expect(page.locator(".app-row").first()).toContainText("YouTube");
  await page.reload();
  await expect(page.locator(".app-row").first()).toContainText("YouTube");
  await page.getByTitle("排序方式").click();
  await page.locator(".application-sort-sheet").getByText("最近使用时间 · 降序", { exact: true }).click();
  await expect(page.locator(".app-row").first()).toContainText("哔哩哔哩");
});

test("TDesign owns the primary mobile controls and overlays", async ({ page }) => {
  await page.goto("/#/overview");
  const serviceSwitch = page.locator(".service-panel .t-switch");
  await expect(serviceSwitch).toBeVisible();
  await expect(page.locator(".proxy-switch")).toHaveCount(0);
  const serviceTopBefore = await page.locator(".service-panel").evaluate((element) => element.getBoundingClientRect().top);
  await serviceSwitch.click();
  const operationMessage = page.locator(".operation-message.t-message");
  await expect(operationMessage).toContainText("代理已启动");
  await expect(page.locator(".t-notice-bar")).toHaveCount(0);
  await expect(operationMessage).toHaveCSS("position", "fixed");
  const serviceTopAfter = await page.locator(".service-panel").evaluate((element) => element.getBoundingClientRect().top);
  expect(Math.abs(serviceTopAfter - serviceTopBefore)).toBeLessThanOrEqual(0.5);
  await expect(operationMessage).not.toBeVisible({ timeout: 4_000 });
  await expect(page.locator(".t-tab-bar")).toBeVisible();
  await expect(page.locator(".bottom-nav")).toHaveCount(0);
  await page.locator(".t-tab-bar").getByText("应用", { exact: true }).click();
  await expect(page).toHaveURL(/#\/applications$/);
  await expect(page.locator(".application-search input")).toBeVisible();
  await expect(page.locator(".filter-bar .t-search")).toHaveCount(0);

  await page.goto("/#/subscriptions");
  await page.locator(".subscription-add-fab").click();
  await expect(page.locator(".t-popup--bottom")).toBeVisible();
  await expect(page.locator(".editor-panel")).toHaveCount(0);
  await page.locator(".subscription-editor input").nth(1).fill("https://canary.invalid/private");
  await page.locator(".subscription-editor button").filter({ hasText: "取消" }).click();
  await expect(page.locator(".subscription-editor")).toHaveCount(0);
});
