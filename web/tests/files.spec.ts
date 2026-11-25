import { test, expect, type Page } from "@playwright/test";
import { mockApi } from "./fixtures";
const headers = {
  "access-control-allow-origin": "http://127.0.0.1:8780",
  "access-control-allow-credentials": "true",
};
async function setup(page: Page, role = "platform_admin") {
  await mockApi(page);
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: { data: { user: { userId: "u", username: "test", role } } },
    }),
  );
  const calls: string[] = [];
  let files = [
    {
      name: "notes.txt",
      path: "/work/notes.txt",
      kind: "file",
      size: 12,
      modifiedAt: 1,
      mode: "0644",
      owner: "1000 / 1000",
      revision: "r1",
    },
  ];
  await page.route("**:8779/api/ops/**", async (r) => {
    const url = new URL(r.request().url());
    calls.push(url.pathname);
    if (!url.pathname.startsWith("/api/ops/local/files"))
      throw Error("Files must not depend on host-monitor: " + url.pathname);
    const path = url.searchParams.get("path") || "/";
    if (url.pathname.endsWith("size"))
      return r.fulfill({
        headers,
        json: {
          data: { bytes: 4096, files: 2, directories: 1, skippedLinks: 0 },
        },
      });
    if (url.pathname.endsWith("properties")) {
      const entry = files.find((f) => f.path === path);
      return r.fulfill({
        headers,
        json: {
          data: {
            ...entry,
            absolutePath: `/current-environment${path}`,
            createdAt: 1,
          },
        },
      });
    }
    if (
      url.pathname.endsWith("download") &&
      url.searchParams.get("check") === "true"
    )
      return r.fulfill({ headers, json: { data: { readable: true } } });
    if (url.pathname.endsWith("download"))
      return r.fulfill({
        headers: {
          ...headers,
          "content-disposition": "attachment; filename=notes.txt",
        },
        contentType: "application/octet-stream",
        body: "hello <script>bad()</script>",
      });
    if (url.pathname.endsWith("preview"))
      return r.fulfill({
        headers,
        json: { data: { text: "hello <script>bad()</script>" } },
      });
    if (url.pathname.endsWith("mutate")) {
      const body = r.request().postDataJSON();
      if (body.action === "delete")
        files = files.filter((f) => f.path !== body.path);
      if (body.action === "rename")
        files = files.map((f) =>
          f.path === body.path
            ? { ...f, name: body.name, path: `/work/${body.name}` }
            : f,
        );
      if (body.action === "mkdir")
        files.push({
          name: body.name,
          path: `/work/${body.name}`,
          kind: "directory",
          size: 0,
          modifiedAt: 1,
          mode: "0755",
          owner: "1000 / 1000",
          revision: "new",
        });
      return r.fulfill({ headers, json: { data: { completed: true } } });
    }
    if (url.pathname.endsWith("upload")) {
      files.push({
        name: "upload.txt",
        path: "/work/upload.txt",
        kind: "file",
        size: 5,
        modifiedAt: 1,
        mode: "0644",
        owner: "1000 / 1000",
        revision: "uploaded",
      });
      return r.fulfill({ headers, json: { data: { completed: true } } });
    }
    if (path === "/denied")
      return r.fulfill({
        headers,
        status: 400,
        json: { data: null, message: "当前服务账号没有此文件或目录的访问权限" },
      });
    const entries =
      path === "/"
        ? [
            {
              name: "work",
              path: "/work",
              kind: "directory",
              size: null,
              modifiedAt: 1,
              mode: "0755",
              owner: "1000 / 1000",
              revision: "root",
            },
          ]
        : files;
    const search = url.searchParams.get("search") || "";
    return r.fulfill({
      headers,
      json: {
        data: {
          root: "/current-environment",
          path,
          items: entries.filter((e) => e.name.includes(search)),
          total: entries.length,
          page: 0,
          pageSize: 100,
        },
      },
    });
  });
  return calls;
}
test("当前环境文件浏览、搜索、文本预览和下载不依赖监控服务", async ({
  page,
}) => {
  const calls = await setup(page);
  await page.goto("/ops/files");
  await expect(
    page.getByRole("heading", { name: "文件", exact: true }),
  ).toBeVisible();
  const routeRequests: string[] = [];
  page.on("request", (request) => {
    if (
      request.resourceType() === "document" ||
      new URL(request.url()).searchParams.has("_rsc")
    )
      routeRequests.push(request.url());
  });
  await expect(page.getByText(/当前环境根目录：/)).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "下级目录", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "work", exact: true }).click();
  await expect(page).toHaveURL(/path=%2Fwork/);
  await expect(page.getByRole("list", { name: "当前路径" })).toContainText(
    "work",
  );
  await page.getByLabel("上级目录", { exact: true }).click();
  await expect(page).toHaveURL(/path=%2F&/);
  await page.getByLabel("下级目录", { exact: true }).click();
  await expect(page).toHaveURL(/path=%2Fwork/);
  await expect(
    page.getByRole("button", { name: "下级目录", exact: true }),
  ).toHaveCount(0);
  await page
    .getByRole("list", { name: "当前路径" })
    .getByRole("link", { name: "/", exact: true })
    .click();
  await expect(page).toHaveURL(/path=%2F&/);
  await page.getByLabel("下级目录", { exact: true }).click();
  await expect(page).toHaveURL(/path=%2Fwork/);
  expect(routeRequests).toEqual([]);
  await expect(page.getByRole("grid", { name: "当前目录文件" })).toContainText(
    "notes.txt",
  );
  await page.getByLabel("搜索当前目录").fill("notes");
  await page.getByRole("button", { name: "搜索", exact: true }).click();
  await page
    .getByRole("button", { name: "打开 notes.txt", exact: true })
    .click();
  const preview = page.getByRole("dialog", { name: "文件预览 · notes.txt" });
  await expect(preview).toContainText("<script>bad()</script>");
  await preview.getByRole("button", { name: "关闭", exact: true }).click();
  const download = page.waitForEvent("download");
  await page.getByRole("link", { name: "下载 notes.txt", exact: true }).click();
  expect((await download).suggestedFilename()).toBe("notes.txt");
  await page.reload();
  await expect(page.getByRole("list", { name: "当前路径" })).toContainText(
    "work",
  );
  expect(calls.every((c) => c.startsWith("/api/ops/local/files"))).toBeTruthy();
});
test("上传、新建目录、删除确认及权限失败", async ({ page }) => {
  await setup(page);
  await page.goto("/ops/files?path=%2Fwork");
  await page.getByLabel("选择上传文件").setInputFiles({
    name: "upload.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("hello"),
  });
  await expect(page.getByRole("grid", { name: "当前目录文件" })).toContainText(
    "upload.txt",
  );
  await page.getByRole("button", { name: "新建目录", exact: true }).click();
  const create = page.getByRole("dialog", { name: "新建目录", exact: true });
  await create.getByLabel("名称", { exact: true }).fill("new-folder");
  await create.getByRole("button", { name: "创建", exact: true }).click();
  await expect(page.getByRole("grid", { name: "当前目录文件" })).toContainText(
    "new-folder",
  );
  const row = page.getByRole("row").filter({ hasText: "notes.txt" });
  await row
    .getByRole("button", { name: "更多 notes.txt", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "删除", exact: true }).click();
  const remove = page.getByRole("dialog", {
    name: "删除文件或目录",
    exact: true,
  });
  await expect(remove).toContainText("不可撤销");
  await expect(row).toBeVisible();
  await remove.getByRole("button", { name: "确认删除", exact: true }).click();
  await expect(row).toHaveCount(0);
  await page.getByLabel("跳转路径", { exact: true }).click();
  const jump = page.getByRole("dialog", { name: "跳转路径", exact: true });
  await jump.getByLabel("目标路径").fill("/denied");
  await jump.getByRole("button", { name: "跳转", exact: true }).click();
  await expect(
    page
      .locator('[data-slot="toast"]')
      .filter({ hasText: "没有此文件或目录的访问权限" }),
  ).toBeVisible();
  await expect(page.locator("main")).not.toContainText(
    "没有此文件或目录的访问权限",
  );
  await expect(
    page.getByRole("button", { name: "新建目录", exact: true }),
  ).toBeDisabled();
});
test("非管理员不请求文件接口", async ({ page }) => {
  const calls = await setup(page, "platform_finance");
  await page.goto("/ops/files");
  await expect(
    page.locator('[data-slot="toast"]').filter({ hasText: "没有文件管理权限" }),
  ).toBeVisible();
  expect(calls).toHaveLength(0);
});

test("读取失败通过全局通知去重，计算与表单错误也使用全局通知", async ({
  page,
}) => {
  await setup(page);
  let reads = 0;
  await page.route("**:8779/api/ops/local/files?**", async (route) => {
    if (new URL(route.request().url()).searchParams.get("path") !== "/denied")
      return route.fallback();
    reads++;
    return route.fulfill({
      headers,
      status: 400,
      json: { data: null, message: "目录读取权限不足" },
    });
  });
  await page.goto("/ops/files?path=%2Fdenied");
  const toasts = page.locator('[data-slot="toast"]');
  await expect(toasts.filter({ hasText: "目录读取权限不足" })).toHaveCount(1);
  await page.locator('button[aria-label="刷新当前页面"]').click();
  await expect.poll(() => reads).toBe(2);
  await expect(page.locator('button[aria-label="刷新当前页面"]')).toBeEnabled();
  await expect(toasts.filter({ hasText: "目录读取权限不足" })).toHaveCount(1);
  await expect(page.locator("main")).not.toContainText("目录读取权限不足");
  await page.getByLabel("根目录", { exact: true }).click();
  await page.route("**:8779/api/ops/local/files/size?**", (route) =>
    route.fulfill({
      headers,
      status: 400,
      json: { data: null, message: "子目录无读取权限，无法计算" },
    }),
  );
  await page
    .getByRole("button", { name: "计算目录大小 work", exact: true })
    .click();
  await expect(
    toasts.filter({ hasText: "子目录无读取权限，无法计算" }),
  ).toHaveCount(1);
  await expect(page.locator("main")).not.toContainText(
    "子目录无读取权限，无法计算",
  );
  await expect(
    page.getByRole("row").filter({ hasText: "work" }),
  ).not.toContainText("0 B");
  await page.route("**:8779/api/ops/local/files/mutate", (route) =>
    route.fulfill({
      headers,
      status: 400,
      json: { data: null, message: "同名目录已存在" },
    }),
  );
  await page.getByRole("button", { name: "新建目录", exact: true }).click();
  const modal = page.getByRole("dialog", { name: "新建目录", exact: true });
  await modal.getByLabel("名称", { exact: true }).fill("work");
  await modal.getByRole("button", { name: "创建", exact: true }).click();
  await expect(toasts.filter({ hasText: "同名目录已存在" })).toHaveCount(1);
  await expect(modal).not.toContainText("同名目录已存在");
  await expect(modal.getByLabel("名称", { exact: true })).toHaveValue("work");
  await modal.getByRole("button", { name: "关闭", exact: true }).click();
  await page.getByRole("button", { name: "work", exact: true }).click();
  await page.route("**:8779/api/ops/local/files/download?**", (route) =>
    route.fulfill({
      headers,
      status: 400,
      json: { data: null, message: "下载文件权限不足" },
    }),
  );
  await page.getByRole("link", { name: "下载 notes.txt", exact: true }).click();
  await expect(toasts.filter({ hasText: "下载文件权限不足" })).toHaveCount(1);
  await expect(page).toHaveURL(/\/ops\/files\?/);
});

test("目录按需计算大小，更多菜单提供重命名、路径复制和属性", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const calls = await setup(page);
  await page.goto("/ops/files");
  await expect(
    page.getByRole("columnheader", { name: "类型", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "计算目录大小 work", exact: true }),
  ).toBeVisible();
  expect(calls.some((p) => p.endsWith("/size"))).toBe(false);
  await page
    .getByRole("button", { name: "计算目录大小 work", exact: true })
    .click();
  await expect(page.getByRole("row").filter({ hasText: "work" })).toContainText(
    "4.0 KiB",
  );
  await page.getByRole("button", { name: "work", exact: true }).click();
  await page
    .getByRole("button", { name: "更多 notes.txt", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "属性", exact: true }).click();
  const properties = page.getByRole("dialog", {
    name: "属性 · notes.txt",
    exact: true,
  });
  await expect(properties).toContainText("/current-environment/work/notes.txt");
  await properties.getByRole("button", { name: "关闭", exact: true }).click();
  await page
    .getByRole("button", { name: "更多 notes.txt", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "复制路径", exact: true }).click();
  await expect
    .poll(() => page.evaluate(() => navigator.clipboard.readText()))
    .toBe("/current-environment/work/notes.txt");
  await page
    .getByRole("button", { name: "更多 notes.txt", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "重命名", exact: true }).click();
  const rename = page.getByRole("dialog", { name: "重命名", exact: true });
  await rename.getByLabel("新名称").fill("renamed.txt");
  await rename.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("grid", { name: "当前目录文件" })).toContainText(
    "renamed.txt",
  );
  await expect(
    page.getByRole("grid", { name: "当前目录文件" }),
  ).not.toContainText("notes.txt");
});
