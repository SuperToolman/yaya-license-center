import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

const storageKey = "yaya-operation-sidebar-expanded";
async function mockAdminApi(page: Parameters<typeof mockApi>[0]) {
  await mockApi(page);
  await page.route("**:8779/api/session", (route) =>
    route.fulfill({
      json: {
        message: "",
        data: {
          user: {
            userId: "sidebar-admin",
            username: "admin",
            displayName: "管理员",
            role: "platform_admin",
            organizationId: "platform",
            status: "active",
          },
        },
      },
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
    }),
  );
}

test("菜单展开和收起均有内容过渡，并支持减少动态效果", async ({ page }) => {
  await mockApi(page);
  await page.goto("/customers");
  const group = page
    .getByRole("navigation", { name: "主导航" })
    .locator("details")
    .filter({ hasText: "业务运营" });
  const summary = group.locator("summary");
  const read = () =>
    group.evaluate((el) => {
      const style = getComputedStyle(el, "::details-content");
      return {
        height: parseFloat(style.blockSize),
        opacity: Number(style.opacity),
      };
    });
  await expect.poll(async () => (await read()).height).toBe(0);
  // Slow the same production transition to reliably observe intermediate frames.
  await page.addStyleTag({
    content: ".sidebar-group::details-content { transition-duration: 800ms; }",
  });
  await summary.click();
  await expect.poll(async () => (await read()).opacity).toBeGreaterThan(0);
  const opening = await read();
  expect(opening.opacity).toBeLessThan(1);
  expect(opening.height).toBeGreaterThan(0);
  await expect.poll(async () => (await read()).opacity).toBe(1);
  const expandedHeight = (await read()).height;
  await summary.click();
  await expect.poll(async () => (await read()).opacity).toBeLessThan(1);
  const closing = await read();
  expect(closing.opacity).toBeGreaterThan(0);
  expect(closing.height).toBeLessThan(expandedHeight);
  await expect.poll(async () => (await read()).height).toBe(0);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await expect
    .poll(() =>
      group.evaluate(
        (el) => getComputedStyle(el, "::details-content").transitionProperty,
      ),
    )
    .toBe("none");
  await summary.click();
  await expect.poll(async () => (await read()).opacity).toBe(1);
});

for (const width of [1280, 390]) {
  test(`导航分组默认收起，独立展开并在刷新和导航后恢复 (${width}px)`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await mockAdminApi(page);
    await page.goto("/ai-employees");
    const nav = page.getByRole("navigation", { name: "主导航" });
    await expect(nav.locator("details")).toHaveCount(3);
    await expect(nav.locator("details[open]")).toHaveCount(0);
    await expect(
      nav.getByRole("link", { name: "AI员工", exact: true }),
    ).toBeHidden();
    await expect(
      nav.getByRole("link", { name: "日志系统", exact: true }),
    ).toBeVisible();
    await expect(
      nav
        .locator("details")
        .filter({ hasText: "系统" })
        .getByRole("link", { name: "日志系统" }),
    ).toHaveCount(0);

    await nav.locator("summary").filter({ hasText: "平台管理" }).click();
    await nav.locator("summary").filter({ hasText: "业务运营" }).click();
    await expect(nav.locator("details[open]")).toHaveCount(2);
    await expect(
      nav.getByRole("link", { name: "AI员工", exact: true }),
    ).toHaveAttribute("aria-current", "page");
    await expect(
      nav
        .locator("details")
        .filter({ hasText: "平台管理" })
        .getByRole("link")
        .first(),
    ).toHaveText("用户管理");
    await expect
      .poll(() => page.evaluate((key) => localStorage.getItem(key), storageKey))
      .toContain('"/orders":true');
    await page.reload();
    await expect(nav.locator("details[open]")).toHaveCount(2);
    await nav.getByRole("link", { name: "Skills", exact: true }).click();
    await expect(page).toHaveURL(/\/skills$/);
    await expect(nav.locator("details[open]")).toHaveCount(2);

    const summary = nav.locator("summary").filter({ hasText: "平台管理" });
    await summary.focus();
    await summary.press("Enter");
    await expect(
      nav.getByRole("link", { name: "Skills", exact: true }),
    ).toBeHidden();
    await expect
      .poll(() => page.evaluate((key) => localStorage.getItem(key), storageKey))
      .toContain('"/users":false');
    await page.reload();
    await expect(nav.locator("details[open]")).toHaveCount(1);
    await expect(
      nav.getByRole("link", { name: "Skills", exact: true }),
    ).toBeHidden();
    await expect(nav.getByRole("link", { name: "订单中心" })).toBeVisible();
  });
}

for (const unavailable of [false, true]) {
  test(`浏览器缓存${unavailable ? "不可用" : "损坏"}时仍可展开收起`, async ({
    page,
  }) => {
    await page.addInitScript(
      ({ key, unavailable }) => {
        if (unavailable) {
          Storage.prototype.getItem = () => {
            throw new Error("Storage disabled");
          };
          Storage.prototype.setItem = () => {
            throw new Error("Storage disabled");
          };
        } else {
          localStorage.setItem(key, "invalid json");
        }
      },
      { key: storageKey, unavailable },
    );
    await mockAdminApi(page);
    await page.goto("/ai-employees");
    const nav = page.getByRole("navigation", { name: "主导航" });
    await expect(nav.locator("details[open]")).toHaveCount(0);
    const summary = nav.locator("summary").filter({ hasText: "平台管理" });
    await summary.click();
    await expect(
      nav.getByRole("link", { name: "Skills", exact: true }),
    ).toBeVisible();
    await summary.click();
    await expect(
      nav.getByRole("link", { name: "Skills", exact: true }),
    ).toBeHidden();
  });
}
