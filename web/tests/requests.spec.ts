import { test, expect } from "@playwright/test";
import { mockApi } from "./fixtures";

test("非 JSON 错误响应可读，成功重试清除错误", async ({ page }) => {
  await mockApi(page);
  await page.goto("/skills");
  await expect(
    page.getByText("bom-quote", { exact: true }).first(),
  ).toBeVisible();
  await page.route("**:8779/api/skills", (route) =>
    route.fulfill({
      status: 502,
      contentType: "text/html",
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      body: "<html>Bad gateway</html>",
    }),
  );
  await page.locator('button[aria-label="刷新当前页面"]').click();
  await expect(
    page.getByRole("alert").filter({ hasText: "无效响应" }),
  ).toContainText("HTTP 502");
  await expect(
    page.getByText("bom-quote", { exact: true }).first(),
  ).toBeVisible();
  await page.unroute("**:8779/api/skills");
  await page.locator('button[aria-label="刷新当前页面"]').click();
  await expect(page.getByText(/无效响应/)).toHaveCount(0);
});

test("导航离开取消旧查询；新页面不会被慢响应覆盖", async ({ page }) => {
  await mockApi(page);
  await page.goto("/skills");
  await expect(
    page.getByText("bom-quote", { exact: true }).first(),
  ).toBeVisible();
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let reached!: () => void;
  const requestReached = new Promise<void>((resolve) => {
    reached = resolve;
  });
  await page.route("**:8779/api/skills", async (route) => {
    reached();
    await gate;
    await route
      .fulfill({
        status: 200,
        contentType: "application/json",
        headers: {
          "access-control-allow-origin": "http://127.0.0.1:8780",
          "access-control-allow-credentials": "true",
        },
        body: JSON.stringify({ data: [], message: "" }),
      })
      .catch(() => {});
  });
  await page.locator('button[aria-label="刷新当前页面"]').click();
  await requestReached;
  const failed = page.waitForEvent("requestfailed", {
    predicate: (request) => request.url().endsWith(":8779/api/skills"),
  });
  await page
    .getByRole("navigation", { name: "主导航" })
    .locator("summary")
    .filter({ hasText: "系统" })
    .click();
  await page.getByRole("link", { name: "用户管理", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "用户管理", exact: true }),
  ).toBeVisible();
  await failed;
  release();
  await expect(
    page.getByRole("heading", { name: "用户管理", exact: true }),
  ).toBeVisible();
});
