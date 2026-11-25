import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

test("操作列仅两个按钮，下架后才能删除，失败可重试", async ({
  page,
}, testInfo) => {
  await mockApi(page);
  let listed = true;
  let deleted = false;
  let fail = true;
  let deletes = 0;
  await page.route("**/api/application-products**", async (route) => {
    if (route.request().method() === "DELETE") {
      deletes++;
      if (!fail) deleted = true;
      await route.fulfill({
        status: fail ? 409 : 200,
        json: {
          data: null,
          message: fail ? "删除失败，请重试" : "应用商品已删除",
        },
      });
    } else if (route.request().method() === "POST") {
      listed = route.request().postDataJSON().listed;
      await route.fulfill({ json: { data: null } });
    } else
      await route.fulfill({
        json: {
          data: deleted
            ? []
            : [
                {
                  productId: "p1",
                  name: "字典中心",
                  publisherSubject: "c1",
                  listingStatus: listed ? "listed" : "withdrawn",
                  members: [],
                  dependencies: [],
                },
              ],
        },
      });
  });
  await page.goto("/application-products");
  const table = page.getByRole("grid", { name: "应用商品", exact: true });
  await expect(table.getByRole("columnheader")).toHaveText([
    "应用",
    "版本",
    "费用",
    "应用类型",
    "发布客户",
    "上架状态",
    "权益客户",
    "操作",
  ]);
  await expect(table.getByText("已上架", { exact: true })).toBeVisible();
  await expect
    .poll(() =>
      table
        .locator("td")
        .evaluateAll((cells) =>
          cells.every(
            (cell) => getComputedStyle(cell).verticalAlign === "middle",
          ),
        ),
    )
    .toBe(true);
  const row = page.getByRole("row").filter({ hasText: "字典中心" });
  await expect(
    row.getByRole("gridcell").last().getByRole("button"),
  ).toHaveCount(2);
  await row.getByRole("button", { name: "字典中心的更多操作" }).click();
  await expect(
    page.getByRole("menuitem", { name: "删除（请先下架）", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("menuitem", { name: "切换版本", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await row.getByRole("button", { name: "下架", exact: true }).click();
  await expect(
    row.getByRole("button", { name: "上架", exact: true }),
  ).toBeVisible();
  await row.getByRole("button", { name: "字典中心的更多操作" }).click();
  await expect(
    page.getByRole("menuitem", { name: "切换版本", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("menuitem", { name: "销售配置", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("menuitem", { name: "赠送", exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("more-menu.png") });
  await page.getByRole("menuitem", { name: "删除", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "删除 字典中心",
    exact: true,
  });
  await expect(dialog).toBeVisible();
  expect(deletes).toBe(0);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(deletes).toBe(0);
  await row.getByRole("button", { name: "字典中心的更多操作" }).click();
  await page.getByRole("menuitem", { name: "删除", exact: true }).click();
  await dialog.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("删除失败，请重试");
  fail = false;
  await dialog.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(row).toHaveCount(0);
  expect(deletes).toBe(2);
});
