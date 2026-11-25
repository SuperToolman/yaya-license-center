import { expect, test, type Page } from "@playwright/test";
import { mockApi } from "./fixtures";

const record = {
  packageId: "7e375d56-7207-4f4d-8bb0-1192328ec398",
  version: "1.13a",
  filename: "1.13a-7e375d56-7207-4f4d-8bb0-1192328ec398.zip",
  artifactType: "offline-image",
  imageId: "sha256:" + "b".repeat(64),
  imagePlatform: "linux/amd64",
  builtAt: "2026-11-09T00:00:00Z",
  uploadedAt: 1794182400000,
  uploadedBy: "usr-admin",
  uploadedByName: "平台管理员张三",
  size: 10 * 1024 * 1024,
  fileCount: 2,
  sha256: "a".repeat(64),
};
async function setup(
  page: Page,
  options: {
    denied?: boolean;
    uploadError?: boolean;
    listError?: boolean;
  } = {},
) {
  await mockApi(page);
  const headers = {
    "access-control-allow-origin": "http://127.0.0.1:8780",
    "access-control-allow-credentials": "true",
  };
  await page.route("**:8779/api/session", (route) =>
    route.fulfill({
      headers,
      json: {
        message: "",
        data: {
          user: {
            userId: "admin",
            username: "admin",
            displayName: "管理员",
            role: options.denied ? "platform_operator" : "platform_admin",
            status: "active",
            organizationId: "platform",
          },
        },
      },
    }),
  );
  const requests: string[] = [];
  let uploaded = false;
  await page.route("**:8779/api/platform-versions", async (route) => {
    const method = route.request().method();
    requests.push(method);
    if (method === "POST") {
      expect(route.request().headers()["content-type"]).toContain(
        "multipart/form-data;",
      );
      if (options.uploadError)
        return route.fulfill({
          headers,
          status: 400,
          json: { message: "文件 SHA-256 不匹配", data: null },
        });
      uploaded = true;
      return route.fulfill({
        headers,
        json: { message: "上传成功", data: record },
      });
    }
    if (options.listError)
      return route.fulfill({
        headers,
        status: 500,
        json: { message: "存储不可用", data: null },
      });
    return route.fulfill({
      headers,
      json: { message: "", data: uploaded ? [record] : [] },
    });
  });
  return requests;
}

test("统一平台管理导航、上传成功刷新列表、搜索和下载链接", async ({ page }) => {
  const requests = await setup(page);
  await page.goto("/orders");
  const nav = page.getByRole("navigation", { name: "主导航" });
  await expect(page.getByLabel("工作区切换")).toHaveCount(0);
  await expect(nav.getByText("应用产品", { exact: true })).toHaveCount(0);
  await expect(nav.getByRole("link", { name: "平台版本管理" })).toBeHidden();
  await nav.locator("summary").filter({ hasText: "平台管理" }).click();
  await nav.getByRole("link", { name: "平台版本管理" }).click();
  await expect(
    page.getByText("暂无平台版本包", { exact: false }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "上传平台包", exact: true }),
  ).toBeEnabled();
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "上传平台包", exact: true }).click();
  await (
    await chooser
  ).setFiles({
    name: record.filename,
    mimeType: "application/zip",
    buffer: Buffer.from("fake-zip-interaction-fixture"),
  });
  const upload = page.getByRole("dialog", { name: "上传平台版本包" });
  await expect(upload).toContainText("平台版本 1.13a 上传成功");
  await expect(upload).toContainText("已耗时");
  await expect(upload).toContainText("预计剩余时间");
  await upload
    .getByRole("button", { name: "关闭", exact: true })
    .last()
    .click();
  const grid = page.getByRole("grid", { name: "平台版本包" });
  for (const column of [
    "版本",
    "包大小",
    "镜像架构",
    "上传人",
    "构建时间",
    "上传时间",
    "操作",
  ]) {
    await expect(
      grid.getByRole("columnheader", { name: column, exact: true }),
    ).toBeVisible();
  }
  await expect(grid).toContainText(record.uploadedByName);
  await expect(grid).not.toContainText(record.uploadedBy);
  await expect(grid).not.toContainText(record.sha256);
  await expect(grid).not.toContainText(record.imageId);
  await expect(grid).toContainText(record.imagePlatform);
  await page
    .getByRole("button", { name: `详情 ${record.version} ${record.packageId}` })
    .click();
  const detail = page.getByRole("dialog");
  await expect(detail).toContainText("离线镜像包");
  await expect(detail).toContainText(record.sha256);
  await expect(detail).toContainText(record.imageId);
  await expect(detail).toContainText("ZIP 文件 SHA-256");
  await expect(detail).toContainText("Docker 镜像标识");
  await detail.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(
    page.getByRole("link", {
      name: `下载 ${record.version} ${record.packageId}`,
    }),
  ).toHaveAttribute(
    "href",
    `http://127.0.0.1:8779/api/platform-versions/${record.packageId}/download`,
  );
  expect(requests).toEqual(["GET", "POST", "GET"]);
  await page.getByLabel("搜索平台版本").fill("missing");
  await expect(page.getByText("没有匹配的平台包。")).toBeVisible();
  await page.getByLabel("搜索平台版本").fill(record.packageId);
  await expect(
    page.getByRole("grid", { name: "平台版本包", exact: true }),
  ).toContainText(record.version);
  await page.reload();
  await expect(nav.getByRole("link", { name: "平台版本管理" })).toBeVisible();
});

test("上传校验失败保留所选文件以便重试", async ({ page }) => {
  const requests = await setup(page, { uploadError: true });
  await page.goto("/platform-versions");
  await page.getByLabel("选择平台版本包").setInputFiles({
    name: "bad.zip",
    mimeType: "application/zip",
    buffer: Buffer.from("bad"),
  });
  await expect(
    page.getByRole("alert").filter({ hasText: "SHA-256 不匹配" }),
  ).toBeVisible();
  const upload = page.getByRole("dialog", { name: "上传平台版本包" });
  await expect(upload).toContainText("bad.zip");
  await expect(
    upload.getByRole("button", { name: "重新上传", exact: true }),
  ).toBeEnabled();
  expect(requests).toEqual(["GET", "POST"]);
  await upload.getByRole("button", { name: "重新上传", exact: true }).click();
  await expect
    .poll(() => requests.filter((method) => method === "POST").length)
    .toBe(2);
});

test("非管理员直达页面不请求平台包", async ({ page }) => {
  const requests = await setup(page, { denied: true });
  await page.goto("/platform-versions");
  await expect(
    page.getByRole("alert").filter({ hasText: "仅平台超级管理员" }),
  ).toBeVisible();
  expect(requests).toEqual([]);
});

test("传输完成后在模态框等待服务器校验，异常响应也在模态框显示", async ({
  page,
}) => {
  await setup(page);
  let finish: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    finish = resolve;
  });
  // Playwright route interception holds the request before Chromium emits
  // upload events. Supply the upload-complete event while delaying the reply.
  await page.addInitScript(() => {
    const send = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function (body) {
      send.call(this, body);
      setTimeout(
        () => this.upload.dispatchEvent(new ProgressEvent("load")),
        100,
      );
    };
  });
  await page.route("**:8779/api/platform-versions", async (route) => {
    if (route.request().method() !== "POST") return route.fallback();
    await gate;
    return route.fulfill({
      status: 500,
      contentType: "text/plain",
      body: "Internal Server Error",
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
    });
  });
  await page.goto("/platform-versions");
  await page.getByLabel("选择平台版本包").setInputFiles({
    name: "test.zip",
    mimeType: "application/zip",
    buffer: Buffer.alloc(65536),
  });
  const modal = page.getByRole("dialog", { name: "上传平台版本包" });
  await expect(modal.getByRole("status")).toHaveText("服务器正在校验");
  await expect(modal).toContainText("校验时长暂无法预估");
  await expect(modal.getByRole("button", { name: "取消上传" })).toBeEnabled();
  finish();
  await expect(modal.getByRole("alert")).toContainText("HTTP 500");
  await expect(modal.getByRole("button", { name: "重新上传" })).toBeEnabled();
});

test("手机显示读取失败与刷新，页面不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await setup(page, { listError: true });
  await page.goto("/platform-versions");
  await expect(
    page.getByRole("alert").filter({ hasText: "存储不可用" }),
  ).toBeVisible();
  await expect(page.getByText("版本包读取失败，请刷新重试。")).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/platform-versions-mobile.png",
    fullPage: true,
  });
});
