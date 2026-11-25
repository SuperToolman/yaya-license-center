import { test, expect, type Page } from "@playwright/test";
import { employee, mockApi } from "./fixtures";

const nativeButton = (page: Page, label: string) =>
  page.locator(`button[aria-label="${label}"]`);

for (const [route, title] of [
  ["/orders", "订单中心"],
  ["/licenses", "客户管理"],
  ["/customers", "客户管理"],
  ["/finance", "财务管理"],
  ["/application-products", "应用管理"],
  ["/providers", "服务商管理"],
  ["/commissions", "提成与返利"],
  ["/users", "用户管理"],
  ["/logs", "日志信息"],
  ["/skills", "Skills 管理"],
  ["/ai-employees", "AI 员工管理"],
  ["/provider-portal", "服务商工作台"],
]) {
  test(`路由渲染 ${route}`, async ({ page }) => {
    await mockApi(page);
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(route);
    await expect(
      page.getByRole("heading", { name: title, exact: true }),
    ).toBeVisible();
    expect(errors).toEqual([]);
  });
}

test("员工保存失败保留输入，重试成功才关闭；快速点击只提交一次", async ({
  page,
}) => {
  const state = await mockApi(page);
  state.employees.push(employee("d", "active", ["bom-quote", "another-skill"]));
  await page.goto("/ai-employees");
  await expect(page.getByText("another-skill", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "新增 AI 员工", exact: true }).click();
  const dialog = page.getByRole("dialog", {
    name: "新增 AI 员工",
    exact: true,
  });
  await dialog.getByLabel("展示名称", { exact: true }).fill("新员工");
  await dialog.getByLabel("分类", { exact: true }).fill("通用");
  await dialog.getByLabel("销售价格", { exact: true }).fill("100");
  state.fail = "/api/ai-employees";
  await dialog.locator('button[type="submit"]').click();
  await expect(dialog.getByText("模拟请求失败", { exact: true })).toBeVisible();
  await expect(dialog.getByLabel("展示名称", { exact: true })).toHaveValue(
    "新员工",
  );
  state.fail = "";
  state.delay = 400;
  await dialog.locator('button[type="submit"]').dblclick();
  await expect(dialog).toHaveCount(0);
  expect(
    state.calls.filter(
      (call) => call.method === "POST" && call.path === "/api/ai-employees",
    ),
  ).toHaveLength(2);
  const saved = state.calls
    .filter(
      (call) => call.method === "POST" && call.path === "/api/ai-employees",
    )
    .at(-1)!;
  expect(saved.body).toMatchObject({
    title: "新员工",
    priceCents: 10000,
    billingCycle: "year",
    applicationIds: [],
  });
});

test("员工能力可选且编辑保留历史应用字段", async ({ page }) => {
  const state = await mockApi(page);
  state.employees[0].applicationIds = ["legacy-app"];
  await page.goto("/ai-employees");
  const row = page.getByRole("row").filter({ hasText: "员工 a" });
  await row.locator('button[aria-label="编辑"]').click();
  const dialog = page.getByRole("dialog", { name: "编辑 AI 员工", exact: true });
  await expect(dialog.getByRole("heading", { name: "员工能力", exact: true })).toBeVisible();
  await dialog.getByText("查看单项能力", { exact: true }).click();
  await dialog.getByText("修改表单设计", { exact: true }).click();
  await expect(dialog.getByRole("checkbox", { name: /修改表单设计/ })).toBeChecked();
  await dialog.locator('button[type="submit"]').click();
  await expect(dialog).toHaveCount(0);
  const saved = state.calls.filter(call => call.method === "POST" && call.path === "/api/ai-employees").at(-1)!;
  expect(saved.body).toMatchObject({ allowedTools: ["yaya_apply_design_patch"], applicationIds: ["legacy-app"] });
});

test("员工重复切换阻止重复提交，失败时保持原状态", async ({ page }) => {
  const state = await mockApi(page);
  await page.goto("/ai-employees");
  const row = page.getByRole("row").filter({ hasText: "员工 a" });
  await expect(row).toBeVisible();
  state.fail = "/api/ai-employees/a/status";
  await row.locator('button[aria-label="下架"]').click();
  await expect(page.locator('p[role="alert"]')).toHaveText("模拟请求失败");
  await expect(row.locator('button[aria-label="下架"]')).toBeVisible();
  state.fail = "";
  state.delay = 400;
  await row.locator('button[aria-label="下架"]').dblclick();
  await expect(row.locator('button[aria-label="上架"]')).toBeVisible();
  expect(
    state.calls.filter((call) => call.path === "/api/ai-employees/a/status"),
  ).toHaveLength(2);
});

test("删除资格重新检查并清除旧错误", async ({ page }) => {
  const state = await mockApi(page);
  state.fail = "/api/ai-employees/a/deletion-eligibility";
  await page.goto("/ai-employees");
  const row = page.getByRole("row").filter({ hasText: "员工 a" });
  await row.locator('button[aria-label="删除"]').click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toContainText("模拟请求失败");
  await expect(dialog.getByRole("button", { name: "确认删除" })).toBeDisabled();
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  state.fail = "";
  await row.locator('button[aria-label="删除"]').click();
  await expect(dialog).toContainText("确认删除“员工 a”吗？");
  await expect(dialog).not.toContainText("模拟请求失败");
  expect(
    state.calls.filter((call) => call.path.endsWith("a/deletion-eligibility")),
  ).toHaveLength(2);
});

test("Skill ZIP 上传契约与编辑弹窗", async ({ page }) => {
  const state = await mockApi(page);
  await page.goto("/skills");
  await expect(
    page.getByText("bom-quote", { exact: true }).first(),
  ).toBeVisible();
  await page.getByRole("button", { name: "导入", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "导入技能" })).toBeVisible();
  await page
    .locator('input[type="file"]')
    .first()
    .setInputFiles({
      name: "skill.zip",
      mimeType: "application/zip",
      buffer: Buffer.from("test zip"),
    });
  await expect
    .poll(() =>
      state.calls.some(
        (call) => call.path === "/api/skills/import" && call.method === "POST",
      ),
    )
    .toBe(true);
  await expect(page.getByRole("dialog", { name: "导入技能" })).toHaveCount(0);
  await nativeButton(page, "编辑 bom-quote").click();
  await expect(
    page.getByRole("dialog", { name: "编辑 Skill", exact: true }),
  ).toBeVisible();
});

test("订单保存、回款、签发保持接口契约", async ({ page }) => {
  const state = await mockApi(page);
  await page.goto("/orders");
  await nativeButton(page, "编辑订单").click();
  let dialog = page.getByRole("dialog", { name: "编辑订单", exact: true });
  await dialog.getByLabel("订单备注", { exact: true }).fill("保留现有商品");
  await dialog.locator('button[type="submit"]').click();
  await expect(dialog).toHaveCount(0);
  expect(
    state.calls.find(
      (call) => call.path === "/api/orders/o1" && call.method === "PUT",
    )?.body,
  ).toMatchObject({
    customerId: "c1",
    notes: "保留现有商品",
    items: [{ kind: "platform", quantity: 1, unitPriceCents: 10000 }],
  });
  await page.getByRole("button", { name: "登记回款", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "登记回款", exact: true });
  state.fail = "/api/orders/o1/payment";
  await dialog.getByRole("button", { name: "确认到账", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText("模拟请求失败");
  await expect(dialog.getByLabel("本次到账（元）", { exact: true })).toHaveValue(
    "100.00",
  );
  state.fail = "";
  await dialog.getByRole("button", { name: "确认到账", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(
    state.calls.filter((call) => call.path === "/api/orders/o1/payment").at(-1)
      ?.body,
  ).toMatchObject({ amountCents: 10000, method: "bank_transfer" });
  state.orders[0].status = "paid";
  state.orders[0].receivedAmountCents = 10000;
  await nativeButton(page, "刷新当前页面").click();
  await page.getByRole("button", { name: "签发许可证", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "从订单签发许可证", exact: true });
  await dialog.getByRole("button", { name: "确认签发", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(
    state.calls.find(
      (call) => call.path === "/api/licenses" && call.method === "POST",
    )?.body,
  ).toEqual({ orderId: "o1" });
});

for (const width of [1440, 768, 390]) {
  test(`弹窗布局隔离 ${width}px`, async ({ page }) => {
    await mockApi(page);
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/ai-employees");
    await page
      .getByRole("row")
      .filter({ hasText: "员工 a" })
      .locator('button[aria-label="编辑"]')
      .click();
    const employeeDialog = page.getByRole("dialog", {
      name: "编辑 AI 员工",
      exact: true,
    });
    await expect(employeeDialog).toBeVisible();
    const rect = await employeeDialog.boundingBox();
    expect(rect!.width).toBeLessThanOrEqual(width);
    expect(rect!.x).toBeGreaterThanOrEqual(0);
    await employeeDialog
      .getByRole("button", { name: "更新头像", exact: true })
      .click();
    const avatarDialog = page.getByRole("dialog", {
      name: "设置头像",
      exact: true,
    });
    await expect(avatarDialog).toBeVisible();
    await expect
      .poll(async () =>
        Number(
          await avatarDialog.evaluate((el) =>
            getComputedStyle(el).width.replace("px", ""),
          ),
        ),
      )
      .toBeLessThanOrEqual(Math.min(width, 460));
    await avatarDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();
    await employeeDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();
    await page.goto("/customers");
    await page.getByRole("button", { name: "新增客户", exact: true }).click();
    const customerDialog = page.getByRole("dialog", {
      name: "新增客户",
      exact: true,
    });
    await expect(customerDialog).toBeVisible();
    expect((await customerDialog.boundingBox())!.width).toBeLessThanOrEqual(
      width,
    );
    await customerDialog
      .getByRole("button", { name: "关闭", exact: true })
      .click();
    await page.goto("/licenses");
    await nativeButton(page, "查看许可证详情").click();
    await expect(
      page.getByRole("dialog", { name: "许可证详情", exact: true }),
    ).toBeVisible();
    await page
      .getByRole("dialog", { name: "许可证详情", exact: true })
      .getByRole("button", { name: "关闭", exact: true })
      .click();
    await page.goto("/orders");
    await nativeButton(page, "编辑订单").click();
    const orderDialog = page.getByRole("dialog", {
      name: "编辑订单",
      exact: true,
    });
    await expect(orderDialog).toBeVisible();
    await expect
      .poll(async () =>
        Number(
          await orderDialog.evaluate((el) =>
            getComputedStyle(el).width.replace("px", ""),
          ),
        ),
      )
      .toBeLessThanOrEqual(width);
    await orderDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();
    await page.getByRole("button", { name: "登记回款", exact: true }).click();
    const paymentDialog = page.getByRole("dialog", {
      name: "登记回款",
      exact: true,
    });
    await expect(paymentDialog).toBeVisible();
    await expect
      .poll(async () =>
        Number(
          await paymentDialog.evaluate((el) =>
            getComputedStyle(el).width.replace("px", ""),
          ),
        ),
      )
      .toBeLessThanOrEqual(width);
    await paymentDialog
      .getByRole("button", { name: "取消", exact: true })
      .click();
    await page.goto("/skills");
    await nativeButton(page, "编辑 bom-quote").click();
    const skillDialog = page.getByRole("dialog", {
      name: "编辑 Skill",
      exact: true,
    });
    await expect(skillDialog).toBeVisible();
    await expect
      .poll(async () =>
        Number(
          await skillDialog.evaluate((el) =>
            getComputedStyle(el).width.replace("px", ""),
          ),
        ),
      )
      .toBeLessThanOrEqual(width);
  });
}
