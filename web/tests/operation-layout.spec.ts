import { test, expect } from "@playwright/test";
import { mockApi } from "./fixtures";

test("订单弹窗使用宽屏布局并适配手机", async ({ page }) => {
  await mockApi(page);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("/orders");
  await page.getByRole("button", { name: "新增订单", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "新增订单", exact: true });
  await expect(dialog).toBeVisible();
  await expect.poll(async () => (await dialog.boundingBox())!.width).toBe(1120);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect
    .poll(async () => (await dialog.boundingBox())!.width)
    .toBeLessThanOrEqual(358);
  await expect(dialog.getByRole("button", { name: "关闭" })).toBeVisible();
});

test("许可证筛选文字可读，详情按授权类型展示", async ({ page }) => {
  await mockApi(page);
  await page.goto("/licenses");
  const filters = page.getByRole("group", { name: "许可证状态筛选" });
  const all = filters.getByRole("button", { name: "全部 1", exact: true });
  await expect(all).toHaveAttribute("aria-pressed", "true");
  const foreground = await all.evaluate((el) => getComputedStyle(el).color);
  expect(foreground).not.toBe("rgb(255, 255, 255)");
  await filters.getByRole("button", { name: "已过期 0", exact: true }).click();
  await expect(
    page.getByText("暂无符合条件的许可证。", { exact: true }),
  ).toBeVisible();
  await all.click();
  await page.locator('button[aria-label="查看许可证详情"]').click();
  const dialog = page.getByRole("dialog", { name: "许可证详情", exact: true });
  await expect(
    dialog.getByRole("region", { name: "许可证基础信息" }),
  ).toContainText("TEST-001");
  await expect(
    dialog.getByRole("region", { name: "模块授权", exact: true }),
  ).toContainText("低代码平台");
  await expect(
    dialog.getByRole("region", { name: "AI 员工授权", exact: true }),
  ).toContainText("无 AI 员工授权");
  await dialog.getByText("完整许可证", { exact: true }).click();
  await expect(dialog.getByRole("textbox", { name: "完整许可证" })).toHaveValue(
    "test",
  );
});

test("财务流水组合筛选、金额合计、空态和重置", async ({ page }) => {
  await mockApi(page);
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.route("**/api/transactions", async (route) =>
    route.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        data: [
          {
            transactionId: "t1",
            customerId: "c1",
            customerName: "客户甲",
            orderId: "o1",
            orderNo: "ORDER-A",
            amountCents: 12300,
            method: "银行转账",
            reference: "REF-A",
            notes: "首款",
            occurredAt: 1794182400,
          },
          {
            transactionId: "t2",
            customerId: "c2",
            customerName: "客户乙",
            orderId: "o2",
            orderNo: "ORDER-B",
            amountCents: 5000,
            method: "现金",
            reference: "REF-B",
            notes: "尾款",
            occurredAt: 1794182400,
          },
        ],
      },
    }),
  );
  await page.goto("/finance");
  await expect(
    page.getByRole("row").filter({ hasText: "ORDER-B" }),
  ).toBeVisible();
  const controls = [
    page.getByRole("button", { name: /客户筛选/ }),
    page.getByRole("button", { name: /收款方式/ }),
    page.locator('[data-slot="date-input-group"]'),
    page.locator('[data-slot="search-field-group"]'),
    page.getByRole("button", { name: "重置筛选" }),
  ];
  const bounds = await Promise.all(
    controls.map((control) => control.boundingBox()),
  );
  expect(
    Math.max(...bounds.map((box) => box!.y)) -
      Math.min(...bounds.map((box) => box!.y)),
  ).toBeLessThanOrEqual(2);
  await page.screenshot({
    path: "test-results/finance-filter-review.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: /客户筛选/ }).click();
  await page.getByRole("option", { name: "客户甲", exact: true }).click();
  await expect(
    page.getByRole("row").filter({ hasText: "ORDER-B" }),
  ).toHaveCount(0);
  await expect(page.getByText(/筛选合计 ¥123.00/)).toBeVisible();
  await page.getByRole("searchbox", { name: "搜索流水" }).fill("尾款");
  await expect(
    page.getByText("暂无符合筛选条件的收款流水。", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "重置筛选" }).click();
  await expect(
    page.getByRole("row").filter({ hasText: "ORDER-B" }),
  ).toBeVisible();
  await page.getByRole("button", { name: /收款方式/ }).click();
  await page.getByRole("option", { name: "现金", exact: true }).click();
  await expect(
    page.getByRole("row").filter({ hasText: "ORDER-A" }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "重置筛选" }).click();
  async function setDate(slot: "start" | "end", date: string) {
    const [year, month, day] = date.split("-");
    const input = page.locator(
      `[data-slot="date-input-group-input"][slot="${slot}"]`,
    );
    for (const [type, value] of [
      ["year", year],
      ["month", month],
      ["day", day],
    ]) {
      await input.locator(`[data-type="${type}"]`).pressSequentially(value);
    }
  }
  await setDate("start", "2026-11-09");
  await setDate("end", "2026-11-09");
  await page.getByRole("button", { name: /选择收款日期范围/ }).click();
  await expect(page.locator('[data-slot="range-calendar"]')).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("row").filter({ hasText: "ORDER-A" }),
  ).toBeVisible();
  await setDate("start", "2026-11-10");
  await setDate("end", "2026-11-01");
  await expect(page.locator("p[role=alert]")).toHaveText(
    "开始日期不能晚于结束日期。",
  );
  await page.getByRole("button", { name: "重置筛选" }).click();
  await expect(page.locator("p[role=alert]")).toHaveCount(0);
  await expect(
    page.getByRole("row").filter({ hasText: "ORDER-B" }),
  ).toBeVisible();
});
