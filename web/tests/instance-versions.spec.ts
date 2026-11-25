import { test, expect } from "@playwright/test";
import { mockApi } from "./fixtures";
const headers = {
  "access-control-allow-origin": "http://127.0.0.1:8780",
  "access-control-allow-credentials": "true",
};

test("客户实例可在 Docker 不可用时下载桌面端安装包", async ({ page }) => {
  await mockApi(page);
  await page.route("**:8779/api/session", (route) =>
    route.fulfill({ headers, json: { data: { user: { userId: "u", username: "admin", role: "platform_admin" } } } }),
  );
  await page.route("**:8779/api/customers/c1/authorization", (route) =>
    route.fulfill({ headers, json: { data: null } }),
  );
  await page.route("**:8779/api/customers/c1/platform-instances", (route) =>
    route.fulfill({ headers, json: { data: [{
      instanceId: "i",
      name: "测试实例",
      version: "1.13a",
      customerId: "c1",
      packageId: "p1",
      publicUrl: "https://customer.example.com",
      containerId: "a".repeat(64),
      containerName: "demo",
      status: "running",
      runtime: { state: "unknown" },
    }] } }),
  );
  await page.route("**:8779/api/platform-runtime", (route) =>
    route.fulfill({ headers, json: { data: { available: false } } }),
  );
  await page.route("**:8779/api/platform-instances/options", (route) =>
    route.fulfill({ headers, json: { data: { customers: [], packages: [] } } }),
  );
  await page.route("**:8779/api/platform-instances/i/desktop-installer", (route) => {
    expect(route.request().method()).toBe("POST");
    return route.fulfill({
      headers: { ...headers, "access-control-expose-headers": "content-disposition", "content-disposition": 'attachment; filename="yaya-test-x64-setup.exe"' },
      contentType: "application/octet-stream",
      body: Buffer.from("MZ"),
    });
  });
  await page.goto("/customers");
  await page.getByRole("button", { name: "查看详情 测试客户", exact: true }).click();
  await page.getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true }).click();
  const downloadButton = page.getByRole("button", { name: "下载桌面端安装包", exact: true });
  await downloadButton.hover();
  await expect(page.getByRole("tooltip")).toHaveText("下载桌面端安装包");
  const download = page.waitForEvent("download");
  await downloadButton.click();
  expect((await download).suggestedFilename()).toBe("yaya-test-x64-setup.exe");
  await page.getByRole("button", { name: "更多操作 测试实例", exact: true }).click();
  await expect(page.getByRole("menuitem", { name: "部署记录 测试实例" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "下载桌面端安装包" })).toHaveCount(0);
  await expect(page.getByRole("menuitem", { name: "平台运维访问" })).toHaveCount(0);
});

test("客户实例版本预览、恢复点和恢复确认", async ({ page }) => {
  await mockApi(page);
  const id = "a".repeat(64);
  let body: Record<string, unknown> | null = null;
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
    r.fulfill({ headers, json: { data: null } }),
  );
  await page.route("**:8779/api/customers/c1/platform-instances", (r) =>
    r.fulfill({
      headers,
      json: {
        data: [
          {
            instanceId: "i",
            name: "测试实例",
            customerId: "c1",
            packageId: "p1",
            containerId: id,
            containerName: "demo",
            status: "running",
            runtime: { state: "running" },
          },
        ],
      },
    }),
  );
  await page.route("**:8779/api/platform-**", (r) => {
    const path = new URL(r.request().url()).pathname;
    let data: unknown = [];
    if (path.endsWith("runtime")) data = { available: true };
    else if (path.endsWith("options"))
      data = {
        customers: [],
        packages: [
          { id: "p1", name: "1.0" },
          { id: "p2", name: "2.0" },
        ],
      };
    else if (path.endsWith("versions"))
      data = {
        revisions: [
          {
            id: "r1",
            packageId: "p1",
            imageId: "sha256:old",
            containerId: id,
            createdAt: 1,
          },
        ],
        recoveryPoints: [
          {
            id: "point1",
            revisionId: "r1",
            status: "ready",
            size: 1024,
            createdAt: 1,
          },
        ],
        tasks: [],
      };
    else if (path.endsWith("version-preview"))
      data = {
        targetImageId: "sha256:new",
        environment: [
          {
            name: "NEW_KEY",
            current: false,
            target: true,
            required: true,
            action: "added",
          },
        ],
        volumes: [{ key: "extra", destination: "/extra", action: "create" }],
      };
    else if (path.endsWith("version-tasks")) {
      body = r.request().postDataJSON();
      data = { taskId: "task" };
    }
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
  await page
    .getByRole("button", { name: "更多操作 测试实例", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "版本与恢复点", exact: true })
    .click();
  const d = page.getByRole("dialog", {
    name: "测试实例 · 版本与恢复点",
    exact: true,
  });
  await expect(d).toBeVisible();
  await d.getByRole("button", { name: /目标平台版本/ }).click();
  await page.getByRole("option", { name: "2.0 · p2", exact: true }).click();
  await d.getByRole("button", { name: "预览版本变更", exact: true }).click();
  await expect(d.getByRole("grid", { name: "环境变量差异" })).toContainText(
    "NEW_KEY",
  );
  await expect(d.getByRole("grid", { name: "存储空间差异" })).toContainText(
    "/extra",
  );
  await expect(
    d.getByRole("button", { name: "备份并升级", exact: true }),
  ).toBeDisabled();
  await d.getByLabel("升级环境变量", { exact: true }).fill("NEW_KEY=secret");
  await d.getByRole("checkbox").check();
  await d.getByRole("button", { name: "备份并升级", exact: true }).click();
  await expect
    .poll(() => body)
    .toMatchObject({
      kind: "upgrade",
      packageId: "p2",
      expectedContainerId: id,
      environment: { NEW_KEY: "secret" },
      acknowledgeDataRestore: true,
    });
  await d.getByRole("tab", { name: "恢复点", exact: true }).click();
  await d.getByRole("button", { name: "恢复到此版本", exact: true }).click();
  await expect(
    d.getByRole("button", { name: "备份当前版本并恢复", exact: true }),
  ).toBeDisabled();
  await d.getByRole("checkbox").check();
  await d
    .getByRole("button", { name: "备份当前版本并恢复", exact: true })
    .click();
  await expect
    .poll(() => body)
    .toMatchObject({
      kind: "restore",
      recoveryPointId: "point1",
      acknowledgeDataRestore: true,
    });
  await expect(page).toHaveURL(/\/customers$/);
});
