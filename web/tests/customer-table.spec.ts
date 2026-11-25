import { expect, test } from "@playwright/test";
import { customer, mockApi } from "./fixtures";

test("客户表格字段逐列对应，更多操作通过 HeroUI 气泡菜单展示", async ({
  page,
}) => {
  await mockApi(page);
  let customerName = customer.name;
  await page.route("**:8779/api/customers/c1", async (route) => {
    customerName = route.request().postDataJSON().name;
    return route.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: { data: { ...customer, name: customerName } },
    });
  });
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        data: {
          user: { userId: "a", role: "platform_admin", username: "admin" },
        },
      },
    }),
  );
  await page.route("**/api/customers", (route) =>
    route.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        data: [
          {
            ...customer,
            name: customerName,
            contactPhone: "010-12345678",
            contactEmail: "first@example.com",
          },
          {
            ...customer,
            customerId: "c2",
            name: "仅邮箱客户",
            contactPhone: "",
            contactEmail: "second@example.com",
          },
        ],
      },
    }),
  );
  await page.goto("/customers");
  const table = page.getByRole("grid", { name: "客户档案" });
  await expect(table.getByRole("columnheader")).toHaveText([
    "客户",
    "联系人",
    "联系电话",
    "联系邮箱",
    "订单",
    "状态",
    "操作",
  ]);
  const first = table.getByRole("row").filter({
    has: page.getByRole("rowheader", { name: "测试客户 c1", exact: true }),
  });
  await expect(first.getByRole("gridcell")).toHaveText([
    "张三",
    "010-12345678",
    "first@example.com",
    "1 笔",
    "启用",
    "",
  ]);
  const second = table.getByRole("row").filter({
    has: page.getByRole("rowheader", { name: "仅邮箱客户 c2", exact: true }),
  });
  await expect(second.getByRole("gridcell")).toHaveText([
    "张三",
    "-",
    "second@example.com",
    "0 笔",
    "启用",
    "",
  ]);
  const headers = table.getByRole("columnheader");
  expect((await headers.first().boundingBox())!.width).toBeGreaterThan(
    (await headers.last().boundingBox())!.width,
  );
  expect((await headers.last().boundingBox())!.width).toBeLessThanOrEqual(145);
  const more = first.getByRole("button", {
    name: "测试客户的更多操作",
    exact: true,
  });
  await expect(more).toHaveText("");
  expect((await more.boundingBox())!.width).toBeLessThanOrEqual(40);
  await first
    .getByRole("button", { name: "测试客户的更多操作", exact: true })
    .click();
  const menu = page.getByRole("menu", { name: "测试客户的更多操作" });
  await expect(menu).toBeVisible();
  await expect(
    menu.getByRole("menuitem", { name: "删除客户", exact: true }),
  ).toBeEnabled();
  await expect(menu.getByRole("menuitem")).toHaveCount(1);
  await expect(page.locator('[data-slot="dropdown-popover"]')).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await second
    .getByRole("button", { name: "仅邮箱客户的更多操作", exact: true })
    .click();
  await expect(
    page.getByRole("menu", { name: "仅邮箱客户的更多操作" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  await page.getByRole("heading", { name: "客户管理", exact: true }).hover();
  const details = first.getByRole("button", {
    name: "查看详情 测试客户",
    exact: true,
  });
  await expect(details).toHaveText("");
  await details.click();
  const modal = page.getByRole("dialog", {
    name: "测试客户 · 客户业务",
    exact: true,
  });
  await expect(modal).toBeVisible();
  await expect(
    modal.getByRole("tab", { name: "概览", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    table.getByRole("button", { name: "编辑", exact: true }),
  ).toHaveCount(0);
  await expect(
    modal.getByRole("textbox", { name: "客户名称", exact: true }),
  ).toHaveValue("测试客户");
  await expect(
    modal.getByRole("textbox", { name: "客户名称", exact: true }),
  ).toHaveAttribute("readonly", "");
  await expect(
    modal.getByRole("textbox", { name: "联系电话", exact: true }),
  ).toHaveValue("010-12345678");
  await modal.getByRole("button", { name: "编辑客户", exact: true }).click();
  const editor = page.getByRole("dialog", { name: "编辑客户", exact: true });
  await expect(editor).toBeVisible();
  await editor
    .getByRole("textbox", { name: "客户名称", exact: true })
    .fill("更新后的客户");
  await editor.getByRole("button", { name: "保存客户", exact: true }).click();
  await expect(editor).toBeHidden();
  const updatedModal = page.getByRole("dialog", {
    name: "更新后的客户 · 客户业务",
    exact: true,
  });
  await expect(updatedModal).toBeVisible();
  await expect(
    updatedModal.getByRole("textbox", { name: "客户名称", exact: true }),
  ).toHaveValue("更新后的客户");
});
