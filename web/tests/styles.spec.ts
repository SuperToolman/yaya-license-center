import { test, expect, type Page } from "@playwright/test";
import { mockApi } from "./fixtures";

async function chips(page: Page) {
  return page
    .locator('[data-slot="table-body"] [data-slot="chip"]')
    .evaluateAll((elements) =>
      elements.map((element) => {
        const style = getComputedStyle(element);
        return {
          text: element.textContent,
          class: element.className,
          color: style.color,
          background: style.backgroundColor,
          token: style.getPropertyValue("--chip-fg"),
        };
      }),
    );
}

test("员工上下架只改变自己的状态，Skills 与其他员工颜色稳定", async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.goto("/ai-employees");
  await expect(page.getByText("员工 a", { exact: true })).toBeVisible();
  const before = await chips(page);
  const row = page.getByRole("row").filter({ hasText: "员工 a" });
  await row.locator("button[aria-label=下架]").click();
  await expect(row.locator("button[aria-label=上架]")).toBeVisible();
  const after = await chips(page);
  await testInfo.attach("chip-comparison", {
    body: JSON.stringify({ before, after }, null, 2),
    contentType: "application/json",
  });
  expect(after.filter((item) => item.text === "bom-quote")).toEqual(
    before.filter((item) => item.text === "bom-quote"),
  );
  expect(after[3]).toEqual(before[3]);
  expect(after[1].color).not.toBe(before[1].color);
  await row.locator("button[aria-label=上架]").click();
  await expect.poll(() => chips(page)).toEqual(before);
  await page.reload();
  await expect.poll(() => chips(page)).toEqual(before);
  await page
    .getByRole("navigation", { name: "主导航" })
    .locator("summary")
    .filter({ hasText: "AI 产品" })
    .click();
  await page.getByRole("link", { name: "Skills 管理", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Skills 管理", exact: true }),
  ).toBeVisible();
  await page.getByRole("link", { name: "AI 员工管理", exact: true }).click();
  await expect.poll(() => chips(page)).toEqual(before);
  await expect(page.getByText("未配置", { exact: true })).toBeVisible();
});

test("失败后保留列表，刷新成功清除错误，未知状态为中性色", async ({ page }) => {
  const state = await mockApi(page);
  await page.goto("/ai-employees");
  await expect(page.getByText("员工 a", { exact: true })).toBeVisible();
  state.fail = "/api/ai-employees";
  await page.locator("button[aria-label=刷新当前页面]").click();
  await expect(page.getByText("模拟请求失败", { exact: true })).toBeVisible();
  await expect(page.getByText("员工 a", { exact: true })).toBeVisible();
  state.fail = "";
  await page.locator("button[aria-label=刷新当前页面]").click();
  await expect(page.getByText("模拟请求失败", { exact: true })).toHaveCount(0);
  await page.goto("/application-products");
  await expect(page.getByText("unknown-state", { exact: true })).toBeVisible();
  await expect(page.locator('[data-slot="chip"]')).toHaveClass(/chip--default/);
});
