import { expect, test, type Page } from "@playwright/test";
import { customer, mockApi } from "./fixtures";
import type { CustomerDeletionEligibility } from "../app/lib/operation-types";

async function setup(page: Page) {
  await mockApi(page);
  const state = {
    customers: [customer],
    report: {
      canDelete: false,
      blockers: [
        { kind: "order", label: "订单", count: 1, recordIds: ["o1"] },
        {
          kind: "license",
          label: "许可证",
          count: 1,
          recordIds: ["license-1"],
        },
      ],
    } as CustomerDeletionEligibility,
    failCheck: false,
    conflict: false,
    deletes: 0,
    checks: 0,
  };
  await page.route("**/api/customers**", async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    const headers = {
      "access-control-allow-origin": "http://127.0.0.1:8780",
      "access-control-allow-credentials": "true",
      "access-control-allow-methods": "GET,DELETE,OPTIONS",
    };
    if (request.method() === "OPTIONS")
      return route.fulfill({ status: 204, headers });
    if (path.endsWith("/deletion-eligibility")) {
      state.checks++;
      return route.fulfill({
        headers,
        status: state.failCheck ? 500 : 200,
        json: {
          data: state.failCheck ? null : state.report,
          message: state.failCheck ? "检查失败" : "",
        },
      });
    }
    if (request.method() === "DELETE") {
      state.deletes++;
      await new Promise((resolve) => setTimeout(resolve, 200));
      if (state.conflict) {
        state.report = {
          canDelete: false,
          blockers: [
            {
              kind: "transaction",
              label: "收款流水",
              count: 1,
              recordIds: ["new-payment"],
            },
          ],
        };
        return route.fulfill({
          headers,
          status: 409,
          json: {
            data: null,
            message: "客户新增了收款流水，请管理员先手动清理关联数据。",
          },
        });
      }
      state.customers = [];
      return route.fulfill({
        headers,
        json: {
          data: { canDelete: true, blockers: [] },
          message: "客户已删除",
        },
      });
    }
    if (path === "/api/customers")
      return route.fulfill({ headers, json: { data: state.customers } });
    return route.fallback();
  });
  await page.goto("/customers");
  return state;
}

async function openDelete(page: Page) {
  await page
    .getByRole("button", { name: "测试客户的更多操作", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "删除客户", exact: true }).click();
  return page.getByRole("alertdialog", { name: "删除客户", exact: true });
}

test("关联数据阻止删除，手动清理后重新检查才能确认删除", async ({
  page,
}, testInfo) => {
  const state = await setup(page);
  const dialog = await openDelete(page);
  await expect(dialog).toContainText("请管理员先手动清理");
  await expect(dialog).toContainText("订单：1 条");
  await expect(dialog).toContainText("license-1");
  await expect(
    dialog.getByRole("button", { name: "确认删除", exact: true }),
  ).toBeDisabled();
  expect(state.deletes).toBe(0);
  await page.screenshot({
    path: testInfo.outputPath("customer-deletion-blocked.png"),
    animations: "disabled",
  });
  state.report = { canDelete: true, blockers: [] };
  await dialog.getByRole("button", { name: "重新检查", exact: true }).click();
  await expect(dialog).toContainText("未发现关联业务数据");
  await dialog
    .getByRole("button", { name: "确认删除", exact: true })
    .dblclick();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByRole("rowheader", { name: "测试客户", exact: true }),
  ).toHaveCount(0);
  expect(state.deletes).toBe(1);
  expect(state.checks).toBe(2);
});

test("检查失败禁止删除，重新打开后重新检查且取消不删除", async ({ page }) => {
  const state = await setup(page);
  state.failCheck = true;
  let dialog = await openDelete(page);
  await expect(dialog.getByRole("alert")).toHaveText("检查失败");
  await expect(
    dialog.getByRole("button", { name: "确认删除", exact: true }),
  ).toBeDisabled();
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  state.failCheck = false;
  state.report = { canDelete: true, blockers: [] };
  dialog = await openDelete(page);
  await expect(dialog).toContainText("未发现关联业务数据");
  await expect(dialog.getByRole("alert")).toHaveCount(0);
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  expect(state.deletes).toBe(0);
  expect(state.checks).toBe(2);
});

test("确认时出现新关联数据则保留客户并刷新阻止原因", async ({ page }) => {
  const state = await setup(page);
  state.report = { canDelete: true, blockers: [] };
  state.conflict = true;
  const dialog = await openDelete(page);
  await expect(dialog).toContainText("未发现关联业务数据");
  await dialog.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText(
    "请管理员先手动清理关联数据",
  );
  await expect(dialog).toContainText("new-payment");
  await expect(
    dialog.getByRole("button", { name: "确认删除", exact: true }),
  ).toBeDisabled();
  expect(state.customers).toHaveLength(1);
  expect(state.deletes).toBe(1);
});
