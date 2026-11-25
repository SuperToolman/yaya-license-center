import { expect, test } from "@playwright/test";
import { customer, mockApi } from "./fixtures";

for (const width of [1440, 390]) {
  test(`客户表单标签和布局 ${width}px`, async ({ page }, testInfo) => {
    await mockApi(page);
    await page.setViewportSize({ width, height: 960 });
    await page.goto("/customers");
    await page.getByRole("button", { name: "新增客户", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "新增客户", exact: true });
    const name = dialog.getByRole("textbox", { name: "客户名称", exact: true });
    await expect(name).toBeVisible();
    await expect(name).toHaveAttribute("required", "");
    for (const label of ["联系人", "联系电话", "联系邮箱", "客户备注"]) {
      await expect(
        dialog.getByRole("textbox", { name: label, exact: true }),
      ).toBeVisible();
    }
    await expect
      .poll(async () => (await dialog.boundingBox())!.width)
      .toBe(Math.min(720, width - 32));
    const nameBox = (await name.boundingBox())!;
    const contactBox = (await dialog
      .getByLabel("联系人", { exact: true })
      .boundingBox())!;
    if (width === 390) expect(contactBox.y).toBeGreaterThan(nameBox.y);
    else expect(contactBox.y).toBe(nameBox.y);
    expect(
      await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth),
    ).toBe(true);
    await page.screenshot({ path: testInfo.outputPath("customer-modal.png") });
    await dialog.getByRole("button", { name: "取消", exact: true }).click();
    await expect(dialog).toHaveCount(0);
  });
}

test("客户保存校验、失败保留输入和编辑回填", async ({ page }) => {
  const state = await mockApi(page);
  await page.goto("/customers");
  await page.getByRole("button", { name: "新增客户", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "新增客户", exact: true });
  await dialog
    .getByRole("textbox", { name: "联系电话", exact: true })
    .fill("13800000000");
  await dialog
    .getByRole("textbox", { name: "客户名称", exact: true })
    .fill("   ");
  await dialog.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText("请输入客户名称。");
  expect(
    state.calls.filter(
      (call) => call.path === "/api/customers" && call.method === "POST",
    ),
  ).toHaveLength(0);
  await dialog
    .getByRole("textbox", { name: "客户名称", exact: true })
    .fill(" 新客户 ");
  await dialog.getByLabel("联系人", { exact: true }).fill("李四");
  await dialog
    .getByRole("textbox", { name: "联系电话", exact: true })
    .fill("010-12345678");
  await dialog.getByLabel("联系邮箱", { exact: true }).fill("test@example.com");
  await dialog.getByLabel("客户备注", { exact: true }).fill("第一行\n第二行");
  state.fail = "/api/customers";
  await dialog.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText("模拟请求失败");
  await expect(dialog.getByLabel("客户备注", { exact: true })).toHaveValue(
    "第一行\n第二行",
  );
  state.fail = "";
  await dialog.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(
    state.calls
      .filter(
        (call) => call.path === "/api/customers" && call.method === "POST",
      )
      .at(-1)?.body,
  ).toEqual({
    name: "新客户",
    contactName: "李四",
    contactPhone: "010-12345678",
    contactEmail: "test@example.com",
    notes: "第一行\n第二行",
  });
  await page.locator('button[aria-label="编辑"]').click();
  const edit = page.getByRole("dialog", { name: "编辑客户", exact: true });
  await expect(
    edit.getByRole("textbox", { name: "客户名称", exact: true }),
  ).toHaveValue("测试客户");
  await expect(edit.getByLabel("联系人", { exact: true })).toHaveValue("张三");
  await edit.getByLabel("客户备注", { exact: true }).fill("已更新备注");
  await edit.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(edit).toHaveCount(0);
  expect(
    state.calls.find(
      (call) => call.path === "/api/customers/c1" && call.method === "POST",
    )?.body,
  ).toMatchObject({
    name: "测试客户",
    contactName: "张三",
    notes: "已更新备注",
  });
});

test("新增客户拒绝空白手机号、重复名称和重复手机号", async ({ page }) => {
  const state = await mockApi(page);
  await page.route("**/api/customers", (route) =>
    route.request().method() === "GET"
      ? route.fulfill({
          headers: {
            "access-control-allow-origin": "http://127.0.0.1:8780",
            "access-control-allow-credentials": "true",
          },
          json: {
            data: [
              { ...customer, contactPhone: "13800000000", status: "inactive" },
            ],
          },
        })
      : route.fallback(),
  );
  await page.goto("/customers");
  await expect(
    page.getByRole("rowheader", { name: "测试客户", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "新增客户", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "新增客户", exact: true });
  const name = dialog.getByRole("textbox", { name: "客户名称", exact: true });
  const phone = dialog.getByRole("textbox", { name: "联系电话", exact: true });
  await expect(phone).toHaveAttribute("required", "");
  await name.fill("新客户");
  await phone.fill("   ");
  await dialog.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText(
    "请输入联系电话（手机号）。",
  );
  await name.fill(" 测试客户 ");
  await phone.fill("13900000000");
  await dialog.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText(
    "客户名称已存在，请勿重复创建。",
  );
  await name.fill("新客户");
  await phone.fill(" 13800000000 ");
  await dialog.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText(
    "联系电话（手机号）已存在，请勿重复创建。",
  );
  expect(
    state.calls.filter(
      (call) => call.path === "/api/customers" && call.method === "POST",
    ),
  ).toHaveLength(0);
});
