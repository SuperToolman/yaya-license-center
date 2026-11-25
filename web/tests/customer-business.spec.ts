import { expect, test } from "@playwright/test";
import { mockApi, customer, order } from "./fixtures";
const headers = {
  "access-control-allow-origin": "http://127.0.0.1:8780",
  "access-control-allow-credentials": "true",
};
const license = {
  license: "test-token",
  licenseId: "lic1",
  subject: "c1",
  customerNameSnapshot: "测试客户",
  orderId: "o1",
  linkageStatus: "linked",
  modules: ["platform", "communication"],
  moduleTitles: { platform: "低代码平台", communication: "通讯模型" },
  moduleExpiresAt: {},
  aiEmployees: [],
  aiEmployeeStatuses: {},
  issuedAt: 1,
  expiresAt: 4102444800,
  platformStatus: "unactivated",
  moduleStatuses: {},
};
const auth = {
  authorizationId: "auth1",
  customerId: "c1",
  latestLicenseId: "lic1",
  activeLicenseId: null,
  revision: 1,
  latest: license,
  versions: [{ license, state: "pending", revision: 1 }],
};
test("客户业务在模态框切换订单授权与实例，部署提交绑定当前客户许可证", async ({
  page,
}) => {
  await mockApi(page);
  let instanceBody: Record<string, unknown> | undefined;
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          user: { userId: "u", username: "admin", role: "platform_admin" },
        },
      },
    }),
  );
  await page.route("**:8779/api/customers/c1/authorization", (r) =>
    r.fulfill({ headers, json: { data: auth } }),
  );
  await page.route("**:8779/api/customers/c1/platform-instances", (r) =>
    r.fulfill({ headers, json: { data: [] } }),
  );
  await page.route("**:8779/api/platform-**", (r) => {
    const path = new URL(r.request().url()).pathname;
    let data: unknown = [];
    if (path.endsWith("/options"))
      data = {
        customers: [{ id: "c1", name: customer.name }],
        packages: [{ id: "p1", name: "1.0", environment: [] }],
      };
    else if (path.endsWith("runtime"))
      data = { available: true, message: "Docker ready" };
    else if (path.endsWith("networks"))
      data = {
        items: [
          {
            name: "bridge",
            driver: "bridge",
            subnets: ["172.17.0.0/16"],
            internal: false,
          },
        ],
      };
    else if (
      path === "/api/platform-instances" &&
      r.request().method() === "POST"
    ) {
      instanceBody = r.request().postDataJSON();
      data = { ...instanceBody, instanceId: "i1" };
    } else if (path.endsWith("deploy")) data = "task1";
    return r.fulfill({ headers, json: { data } });
  });
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const modal = page.getByRole("dialog", {
    name: "测试客户 · 客户业务",
    exact: true,
  });
  await expect(modal).toBeVisible();
  await expect(page).toHaveURL(/\/customers$/);
  const initialBounds = await modal.boundingBox();
  const tabList = modal.getByRole("tablist", { name: "客户业务", exact: true });
  const initialTabBounds = await tabList.boundingBox();
  for (const name of ["概览", "订单", "授权", "平台实例"]) {
    await modal.getByRole("tab", { name, exact: true }).click();
    await expect
      .poll(async () => (await modal.boundingBox())!.height)
      .toBe(initialBounds!.height);
    expect((await tabList.boundingBox())!.y).toBe(initialTabBounds!.y);
  }
  const instanceTable = modal.getByRole("grid", {
    name: "平台实例",
    exact: true,
  });
  await expect(instanceTable).toBeVisible();
  const domainHeader = instanceTable.getByRole("columnheader", {
    name: "域名",
    exact: true,
  });
  const authHeader = instanceTable.getByRole("columnheader", {
    name: "授权同步",
    exact: true,
  });
  expect((await domainHeader.boundingBox())!.width).toBeGreaterThanOrEqual(230);
  expect((await authHeader.boundingBox())!.width).toBeGreaterThanOrEqual(130);
  for (const viewport of [
    { width: 1600, height: 1000 },
    { width: 900, height: 600 },
  ]) {
    await page.setViewportSize(viewport);
    await expect
      .poll(async () => {
        const box = (await modal.boundingBox())!;
        return box.height <= viewport.height && box.y >= 0;
      })
      .toBe(true);
    const panel = modal.getByRole("tabpanel");
    const scrollContainer = instanceTable.locator("..");
    expect(
      await scrollContainer.evaluate((el) => el.scrollWidth > el.clientWidth),
    ).toBe(true);
    await panel.evaluate((el) => {
      el.scrollTop = 10000;
    });
    const box = (await modal.boundingBox())!;
    const tabsBox = (await tabList.boundingBox())!;
    expect(tabsBox.y).toBeGreaterThan(box.y);
    expect(tabsBox.y + tabsBox.height).toBeLessThan(box.y + box.height);
  }
  await page.setViewportSize({ width: 1280, height: 720 });

  await modal.getByRole("tab", { name: "授权", exact: true }).click();
  await expect(
    modal.getByText("auth1", { exact: false }).first(),
  ).toBeVisible();
  await expect(
    modal.getByRole("grid", { name: "许可证版本历史" }),
  ).toContainText("待应用");
  await modal.getByRole("tab", { name: "订单", exact: true }).click();
  await expect(modal.getByRole("grid", { name: "订单列表" })).toContainText(
    order.orderNo,
  );
  await modal.getByRole("tab", { name: "平台实例", exact: true }).click();
  await modal.getByRole("button", { name: "新建实例", exact: true }).click();
  const editor = page.getByRole("dialog", {
    name: "新建平台实例",
    exact: true,
  });
  await expect(editor).toContainText("auth1");
  await expect(editor.getByRole("button", { name: /所属客户/ })).toBeDisabled();
  await editor.getByRole("button", { name: /平台版本包/ }).click();
  await page.getByRole("option", { name: "1.0 · p1", exact: true }).click();
  await editor
    .getByLabel("平台域名", { exact: true })
    .fill("https://demo.example.com");
  await editor.getByRole("button", { name: "创建并部署", exact: true }).click();
  await expect(editor).toHaveCount(0);
  expect(instanceBody).toMatchObject({
    customerId: "c1",
    licenseId: "lic1",
    packageId: "p1",
  });
  await expect(modal).toBeVisible();
  await expect(page).toHaveURL(/\/customers$/);
});
test("旧授权入口跳转客户管理，客户授权支持撤销失败重试并更新版本状态", async ({
  page,
}) => {
  await mockApi(page);
  const authorization = structuredClone(auth);
  let failRevoke = true;
  let revokeCalls = 0;
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          user: { userId: "u", role: "platform_admin", username: "admin" },
        },
      },
    }),
  );
  await page.route("**:8779/api/customers/c1/authorization", (r) =>
    r.fulfill({ headers, json: { data: authorization } }),
  );
  await page.route("**:8779/api/licenses/lic1/destroy", (r) => {
    revokeCalls += 1;
    if (failRevoke)
      return r.fulfill({
        status: 500,
        headers,
        json: { data: null, message: "模拟撤销失败" },
      });
    authorization.latest.platformStatus = "destroyed";
    authorization.versions[0].state = "revoked";
    return r.fulfill({ headers, json: { data: null } });
  });
  await page.goto("/licenses");
  await expect(page).toHaveURL(/\/customers$/);
  await expect(
    page.getByRole("heading", { name: "客户管理", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("navigation").locator('a[href="/licenses"]'),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  const modal = page.getByRole("dialog", {
    name: "测试客户 · 客户业务",
    exact: true,
  });
  await modal.getByRole("tab", { name: "授权", exact: true }).click();
  await expect(
    modal.getByRole("grid", { name: "许可证版本历史" }),
  ).toContainText("待应用");
  await modal.locator('button[aria-label="查看许可证"]').click();
  await expect(
    page.getByRole("dialog", { name: "许可证详情", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await modal
    .getByRole("button", { name: "撤销客户授权", exact: true })
    .click();
  const confirmation = page.getByRole("alertdialog");
  await confirmation.getByRole("button", { name: "取消", exact: true }).click();
  expect(revokeCalls).toBe(0);
  await modal
    .getByRole("button", { name: "撤销客户授权", exact: true })
    .click();
  await confirmation.getByRole("button", { name: "确认", exact: true }).click();
  await expect(confirmation.getByRole("alert")).toContainText("模拟撤销失败");
  failRevoke = false;
  await confirmation.getByRole("button", { name: "确认", exact: true }).click();
  await expect(confirmation).toHaveCount(0);
  await expect(
    modal.getByRole("grid", { name: "许可证版本历史" }),
  ).toContainText("已撤销");
  await expect(
    modal.getByRole("button", { name: "撤销客户授权", exact: true }),
  ).toHaveCount(0);
  expect(revokeCalls).toBe(2);
});

test("运营人员在客户授权中只能查看，不能撤销", async ({ page }) => {
  await mockApi(page);
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          user: {
            userId: "u",
            role: "platform_operator",
            username: "operator",
          },
        },
      },
    }),
  );
  await page.route("**:8779/api/customers/c1/authorization", (r) =>
    r.fulfill({ headers, json: { data: auth } }),
  );
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  const modal = page.getByRole("dialog", {
    name: "测试客户 · 客户业务",
    exact: true,
  });
  await modal.getByRole("tab", { name: "授权", exact: true }).click();
  await expect(
    modal.getByRole("grid", { name: "许可证版本历史" }),
  ).toBeVisible();
  await expect(
    modal.getByRole("button", { name: "撤销客户授权", exact: true }),
  ).toHaveCount(0);
});
test("没有授权的客户能查看实例但不能创建", async ({ page }) => {
  await mockApi(page);
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          user: { userId: "u", role: "platform_admin", username: "admin" },
        },
      },
    }),
  );
  await page.route("**:8779/api/customers/c1/authorization", (r) =>
    r.fulfill({ headers, json: { data: null } }),
  );
  await page.route("**:8779/api/customers/c1/platform-instances", (r) =>
    r.fulfill({ headers, json: { data: [] } }),
  );
  await page.route("**:8779/api/platform-**", (r) =>
    r.fulfill({
      headers,
      json: {
        data: r.request().url().endsWith("runtime")
          ? { available: true }
          : { customers: [], packages: [] },
      },
    }),
  );
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = page.getByRole("dialog", { name: "测试客户 · 客户业务" });
  await expect(
    d.getByRole("button", { name: "新建实例", exact: true }),
  ).toBeDisabled();
  await expect(d).toContainText("客户尚无有效平台授权");
});
