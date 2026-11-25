import { test, expect, type Page } from "@playwright/test";
import { mockApi } from "./fixtures";
import type { OperationUser } from "../app/lib/operation-types";

async function usersApi(page: Page, role = "platform_admin") {
  await mockApi(page);
  const state = {
    fail: false,
    users: [
      {
        userId: "admin",
        username: "admin",
        displayName: "管理员",
        role: "platform_admin",
        organizationId: "platform",
        status: "active",
      },
    ] as OperationUser[],
    calls: [] as {
      method: string;
      path: string;
      body: Record<string, string>;
    }[],
  };
  await page.route(
    /http:\/\/(?:127\.0\.0\.1|localhost):8779\/api\/(?:users(?:\/[^?]*)?|session)$/,
    async (route) => {
      const request = route.request();
      const path = new URL(request.url()).pathname;
      const method = request.method();
      const headers = {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
        "access-control-allow-headers": "content-type",
        "access-control-allow-methods": "GET,POST,PUT,DELETE,OPTIONS",
      };
      if (method === "OPTIONS") return route.fulfill({ status: 204, headers });
      const body = request.postData() ? request.postDataJSON() : {};
      state.calls.push({ method, path, body });
      if (state.fail && method !== "GET")
        return route.fulfill({
          status: 500,
          headers,
          json: { data: null, message: "模拟保存失败" },
        });
      let data: unknown = state.users;
      if (path === "/api/session")
        data = { user: { ...state.users[0], role }, authenticated: true };
      else if (method === "POST" && path === "/api/users") {
        const user = { ...body, userId: "new-user", status: "active" };
        state.users.push(user);
        data = user;
      } else if (method !== "GET") {
        const id = path.split("/")[3];
        const user = state.users.find((u) => u.userId === id)!;
        if (method === "DELETE")
          state.users = state.users.filter((u) => u.userId !== id);
        else Object.assign(user, body);
        data = user;
      }
      await route.fulfill({ headers, json: { data, message: "ok" } });
    },
  );
  return state;
}

test("用户新增、编辑、停用、启用和删除完整闭环", async ({ page }) => {
  const state = await usersApi(page);
  await page.goto("/users");
  const adminRow = page
    .getByRole("row")
    .filter({ hasText: "admin（当前账号）" });
  await expect(
    adminRow.getByRole("button", { name: "停用", exact: true }),
  ).toBeDisabled();
  await expect(
    adminRow.getByRole("button", { name: "删除", exact: true }),
  ).toBeDisabled();
  await page.screenshot({
    path: test.info().outputPath("users.png"),
    fullPage: true,
    animations: "disabled",
  });
  await adminRow.getByRole("button", { name: "编辑", exact: true }).click();
  let dialog = page.getByRole("dialog", { name: "编辑用户", exact: true });
  await expect(dialog.getByLabel("角色", { exact: true })).toBeDisabled();
  await expect(dialog.getByLabel("重置密码", { exact: true })).toBeDisabled();
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await page.getByRole("button", { name: "新增用户", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "新增用户", exact: true });
  await dialog.getByLabel("账号", { exact: true }).fill("new.operator");
  await dialog.getByLabel("姓名", { exact: true }).fill("测试运营");
  await dialog.getByLabel("初始密码", { exact: true }).fill("test-password");
  await page.screenshot({
    path: test.info().outputPath("new-user.png"),
    fullPage: true,
    animations: "disabled",
  });
  state.fail = true;
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText("模拟保存失败");
  await expect(dialog.getByLabel("姓名", { exact: true })).toHaveValue(
    "测试运营",
  );
  state.fail = false;
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const row = page.getByRole("row").filter({ hasText: "new.operator" });
  await expect(row).toBeVisible();
  await expect(
    row.getByRole("button", { name: "删除", exact: true }),
  ).toBeDisabled();
  await row.getByRole("button", { name: "编辑", exact: true }).click();
  dialog = page.getByRole("dialog", { name: "编辑用户", exact: true });
  await expect(dialog.getByLabel("账号", { exact: true })).toBeDisabled();
  await dialog.getByLabel("姓名", { exact: true }).fill("财务人员");
  await dialog
    .getByLabel("角色", { exact: true })
    .selectOption("platform_finance");
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  await expect(row.getByText("财务人员", { exact: true })).toBeVisible();
  expect(
    state.calls.find((c) => c.method === "PUT")?.body.password,
  ).toBeUndefined();
  for (const action of ["停用", "启用", "停用", "删除"]) {
    await row.getByRole("button", { name: action, exact: true }).click();
    dialog = page.getByRole("dialog", { name: `${action}用户`, exact: true });
    await dialog
      .getByRole("button", { name: `确认${action}`, exact: true })
      .click();
    await expect(dialog).toHaveCount(0);
  }
  await expect(row).toHaveCount(0);
  expect(state.calls.filter((c) => c.method === "DELETE")).toHaveLength(1);
});

for (const role of ["platform_operator", "platform_finance"]) {
  test(`${role} 只读，无账号维护入口`, async ({ page }) => {
    await usersApi(page, role);
    await page.goto("/users");
    await expect(
      page.getByText("当前角色仅可查看账号。", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole("button", { name: "新增用户", exact: true }),
    ).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "编辑", exact: true }),
    ).toHaveCount(0);
  });
}
