import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

test("导入、销售配置和赠送弹窗在桌面与窄屏保留可见操作区", async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.route("**/api/application-products/p1/commercial", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.route("**/api/application-submissions", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.goto("/application-products");
  const cases = [
    {
      trigger: "导入应用包",
      title: "导入应用包",
      submit: "导入并进入审核",
      image: "import",
    },
    {
      trigger: "销售配置",
      title: "测试应用 · 销售配置",
      submit: "提交商业配置",
      image: "commercial",
    },
    {
      trigger: "赠送",
      title: "赠送 测试应用",
      submit: "确认赠送",
      image: "gift",
    },
  ];
  for (const item of cases) {
    await page.setViewportSize({ width: 1920, height: 1080 });
    if (item.trigger !== "导入应用包") {
      await page.getByRole("button", { name: /的更多操作/ }).click();
      await page
        .getByRole("menuitem", { name: item.trigger, exact: true })
        .click();
    } else
      await page
        .getByRole("button", { name: item.trigger, exact: true })
        .click();
    const dialog = page.getByRole("dialog", { name: item.title, exact: true });
    await expect(dialog).toBeVisible();
    if (item.image === "import") {
      await dialog.getByRole("button", { name: /定价方式/ }).click();
      await page.getByRole("option", { name: "一次买断", exact: true }).click();
    }
    for (const [width, height] of [
      [1920, 1080],
      [1280, 720],
      [390, 844],
    ]) {
      await page.setViewportSize({ width, height });
      await expect(
        dialog.getByRole("button", { name: item.submit, exact: true }),
      ).toBeInViewport();
      await expect
        .poll(() =>
          dialog.evaluate(
            (element) => element.scrollWidth - element.clientWidth,
          ),
        )
        .toBeLessThanOrEqual(1);
      await expect
        .poll(() =>
          dialog.evaluate((element) => element.getBoundingClientRect().right),
        )
        .toBeLessThanOrEqual(width);
      if (width !== 1280)
        await page.screenshot({
          path: testInfo.outputPath(`${item.image}-${width}.png`),
        });
    }
    await dialog
      .getByRole("button", { name: "关闭", exact: true })
      .first()
      .click();
    await expect(dialog).toHaveCount(0);
  }
});

test("赠送失败保留表单，成功刷新商品并关闭弹窗", async ({ page }) => {
  const state = await mockApi(page);
  let fail = true;
  const grants: unknown[] = [];
  await page.route(
    "**/api/application-products/p1/entitlements",
    async (route) => {
      grants.push(route.request().postDataJSON());
      await route.fulfill({
        status: fail ? 500 : 200,
        json: { data: null, message: fail ? "赠送失败，请重试" : "赠送成功" },
      });
    },
  );
  await page.goto("/application-products");
  await page.getByRole("button", { name: /的更多操作/ }).click();
  await page.getByRole("menuitem", { name: "赠送", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "赠送 测试应用" });
  const submit = dialog.getByRole("button", { name: "确认赠送" });
  await expect(submit).toBeDisabled();
  await dialog
    .getByRole("textbox", { name: "许可证主体" })
    .fill(" customer-license ");
  await dialog.getByRole("textbox", { name: "赠送原因" }).fill(" 客户试用 ");
  await submit.click();
  await expect(dialog.getByRole("alert")).toContainText("赠送失败，请重试");
  await expect(dialog.getByRole("textbox", { name: "赠送原因" })).toHaveValue(
    " 客户试用 ",
  );
  const before = state.calls.filter(
    (call) => call.path === "/api/application-products",
  ).length;
  fail = false;
  await submit.click();
  await expect(dialog).toHaveCount(0);
  expect(grants).toEqual([
    { subject: "customer-license", reason: "客户试用" },
    { subject: "customer-license", reason: "客户试用" },
  ]);
  await expect
    .poll(
      () =>
        state.calls.filter((call) => call.path === "/api/application-products")
          .length,
    )
    .toBeGreaterThan(before);
});

test("商业审核在弹窗内填写驳回原因，失败保留输入", async ({ page }) => {
  await mockApi(page);
  let fail = true;
  let status = "pending_review";
  const config = {
    pricingMode: "paid",
    priceCents: 9900,
    publisherRateBasisPoints: 7500,
    description: "应用销售说明",
  };
  await page.route("**/api/application-submissions", (route) =>
    route.fulfill({
      json: {
        data: [
          {
            productId: "p1",
            submissionId: "r1",
            version: "1.0.0",
            status: "approved",
          },
        ],
      },
    }),
  );
  await page.route("**/api/application-products/p1/commercial", (route) =>
    route.fulfill({
      json: { data: [{ id: "commercial1", releaseId: "r1", status, config }] },
    }),
  );
  await page.route(
    "**/api/application-commercial/commercial1/review",
    async (route) => {
      expect(route.request().postDataJSON()).toEqual({
        approved: false,
        reason: "分成比例需要调整",
      });
      if (!fail) status = "rejected";
      await route.fulfill({
        status: fail ? 500 : 200,
        json: { data: null, message: fail ? "审核暂时失败" : "审核完成" },
      });
    },
  );
  await page.goto("/application-products");
  await page.getByRole("button", { name: /的更多操作/ }).click();
  await page.getByRole("menuitem", { name: "销售配置", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "测试应用 · 销售配置" });
  await dialog.getByRole("button", { name: "驳回", exact: true }).click();
  await expect(
    dialog.getByRole("button", { name: "确认驳回", exact: true }),
  ).toBeDisabled();
  await dialog
    .getByRole("textbox", { name: "驳回原因" })
    .fill(" 分成比例需要调整 ");
  await dialog.getByRole("button", { name: "确认驳回", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("审核暂时失败");
  await expect(dialog.getByRole("textbox", { name: "驳回原因" })).toHaveValue(
    " 分成比例需要调整 ",
  );
  fail = false;
  await dialog.getByRole("button", { name: "确认驳回", exact: true }).click();
  await expect(dialog.getByRole("textbox", { name: "驳回原因" })).toHaveCount(
    0,
  );
  await expect(dialog.getByText("已驳回", { exact: true })).toBeVisible();
});
