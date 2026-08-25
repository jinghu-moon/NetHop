import { expect, test } from "@playwright/test";

test.describe("真实设置界面", () => {
  test("首页只提供真实分类入口，二级页读写 schema 字段", async ({ page }) => {
    await page.goto("/#/settings");
    await expect(page.locator(".settings-base h2")).toHaveText("设置");
    for (const title of ["更新与自动选择", "网络接管", "接口范围", "路由策略", "日志", "高级设置"]) {
      await expect(page.locator(".settings-group").filter({ hasText: title }).last()).toContainText(title);
    }
    await expect(page.getByText("network.proxy_tcp", { exact: true })).toHaveCount(0);
    await page.locator(".settings-group").filter({ hasText: "更新与自动选择" }).getByText("更新与自动选择", { exact: true }).click();
    await expect(page).toHaveURL(/#\/settings\/updates$/);
    await expect(page.locator(".settings-secondary-shell h2")).toHaveText("更新与自动选择");
    await expect(page.getByText("活动配置摘要", { exact: true })).toHaveCount(0);
    await expect(page.locator(".settings-field-control").filter({ hasText: "最大自动候选数" })).toBeVisible();
    await expect(page.locator(".settings-field-control .nh-input-number")).toHaveCount(4);
    await expect(page.locator(".settings-field-control").filter({ hasText: "proxy.urltest.max_candidates" })).toHaveCount(0);
    const firstRow = page.locator(".schema-field-wrap").first();
    await expect(firstRow).toHaveCSS("padding-top", "0px");
    await expect(firstRow).toHaveCSS("border-bottom-width", "0px");
    expect((await firstRow.boundingBox())?.height ?? 999).toBeLessThan(60);
    await expect(page.getByText("应用后重新激活代理核心", { exact: true })).toHaveCount(0);
    const divider = await page.locator(".schema-field-wrap").nth(1).evaluate((element) => {
      const style = getComputedStyle(element, "::before");
      return { left: style.left, right: style.right, width: style.borderTopWidth };
    });
    expect(divider).toEqual({ left: "13px", right: "13px", width: "1px" });
  });

  test("一级和二级设置页使用覆盖式滑入并支持返回动画状态", async ({ page }) => {
    await page.goto("/#/settings");
    const base = page.locator(".settings-base");
    const secondary = page.locator(".settings-secondary-shell");
    await expect(base).not.toHaveClass(/settings-base--pushed/);
    await expect(base).toHaveCSS("transition-property", /transform/);
    await expect(base).toHaveCSS("transition-duration", "0.35s");
    await expect(secondary).not.toHaveClass(/settings-secondary-shell--active/);
    await expect(secondary).toHaveCSS("visibility", "hidden");
    await expect(secondary).toHaveCSS("padding-left", "0px");
    await page.locator(".settings-group").filter({ hasText: "更新与自动选择" }).getByText("更新与自动选择", { exact: true }).click();
    await expect(secondary).toBeVisible();
    await expect(secondary).toHaveClass(/settings-secondary-shell--active/);
    await expect(base).toHaveClass(/settings-base--pushed/);
    await expect(secondary).toHaveCSS("transition-property", /transform/);
    await expect(secondary).toHaveCSS("transition-duration", /0\.35s/);
    await secondary.getByRole("button", { name: "返回设置" }).click();
    await expect(page).toHaveURL(/#\/settings$/);
    await expect(base).not.toHaveClass(/settings-base--pushed/);
    await expect(secondary).not.toHaveClass(/settings-secondary-shell--active/);
    await expect(secondary).toHaveCSS("visibility", "hidden");
  });

  test("能力不支持时控件禁用并显示原因，草稿状态不提前变更活动摘要", async ({ page }) => {
    await page.goto("/#/settings/interfaces");
    const hotspot = page.locator(".schema-field-wrap").filter({ hasText: "热点代理" });
    await expect(hotspot.getByRole("switch")).toBeDisabled();
    await expect(hotspot).toContainText("unsupported: hotspot_not_available");
    const wifi = page.locator(".schema-field-wrap").filter({ has: page.getByText("Wi-Fi", { exact: true }) });
    await wifi.getByRole("switch").click();
    await expect(page.locator(".settings-page")).toContainText("有未应用修改");
    await expect(page.locator(".settings-page")).toContainText("活动摘要 0000000000…");
  });

  test("验证和应用通过私有 payload 持久化配置并更新 digest", async ({ page }) => {
    await page.goto("/#/settings/interfaces");
    const wifi = page.locator(".schema-field-wrap").filter({ has: page.getByText("Wi-Fi", { exact: true }) });
    await wifi.getByRole("switch").click();
    await page.locator(".settings-secondary-shell--active").getByRole("button", { name: "验证" }).click();
    await expect(page.locator(".settings-notice--success")).toContainText("更新网络接管计划");
    await page.getByRole("button", { name: "应用配置" }).click();
    await expect(page.locator(".settings-dialog")).toContainText("事务化发布");
    await page.locator(".settings-dialog").getByRole("button", { name: "应用" }).click();
    await expect(page.locator(".settings-page")).toContainText("已同步");
    await expect(page.locator(".settings-page")).toContainText("活动摘要 1000000000…");
    await expect(wifi.getByRole("switch")).not.toBeChecked();
  });
});
