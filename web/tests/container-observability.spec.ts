import { expect, test, type Page } from "@playwright/test";
import { mockApi } from "./fixtures";
const id = "a".repeat(64),
  headers = {
    "access-control-allow-origin": "http://127.0.0.1:8780",
    "access-control-allow-credentials": "true",
  };
async function setup(page: Page) {
  await mockApi(page);
  await page.route("**:8779/api/customers/c1/authorization", (r) =>
    r.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        data: {
          authorizationId: "auth1",
          customerId: "c1",
          latestLicenseId: "lic1",
          activeLicenseId: null,
          revision: 1,
          latest: {
            licenseId: "lic1",
            modules: ["platform"],
            platformStatus: "unactivated",
            expiresAt: 4102444800,
          },
          versions: [],
        },
      },
    }),
  );
  const paths: string[] = [];
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          user: {
            userId: "u",
            username: "admin",
            role: "platform_admin",
            displayName: "管理员",
          },
        },
      },
    }),
  );
  await page.route(
    "**:8779/api/{platform-**,customers/c1/platform-instances}",
    (r) => {
      const path = new URL(r.request().url()).pathname;
      return r.fulfill({
        headers,
        json: {
          data: path.endsWith("runtime")
            ? { available: true }
            : path.endsWith("options")
              ? { customers: [], packages: [] }
              : [
                  {
                    instanceId: "i",
                    name: "测试实例",
                    customerName: "客户 A",
                    containerName: "test-customer",
                    containerId: id,
                    status: "running",
                    runtime: { state: "running", checkedAt: Date.now() },
                    version: "1",
                    packageId: "p",
                  },
                ],
        },
      });
    },
  );
  await page.route("**:8779/api/ops/local/**", (r) => {
    const url = new URL(r.request().url());
    paths.push(url.pathname + url.search);
    let data: unknown = {
      id,
      name: "test-customer",
      image: "yaya:1",
      state: "running",
      ports: [],
      mounts: [],
      terminalAllowed: true,
      allowedActions: ["stop"],
      association: {
        instanceId: "i",
        instanceName: "测试实例",
        customerName: "客户 A",
        active: true,
      },
    };
    if (
      url.pathname.endsWith("/actions") ||
      url.pathname.endsWith("/terminal-sessions")
    )
      data = [];
    if (url.pathname.endsWith("/logs")) {
      if (url.searchParams.has("download"))
        return r.fulfill({
          headers: {
            ...headers,
            "content-disposition": "attachment; filename=test.log",
          },
          contentType: "text/plain",
          body: "stdout ok\nstderr failed",
        });
      data = {
        text: "",
        entries: [
          {
            stream: "stdout",
            timestamp: "2026-11-10T00:00:00Z",
            text: "service ready",
          },
          {
            stream: "stderr",
            timestamp: "2026-11-10T00:00:01Z",
            text: "request failed",
          },
        ],
      };
    }
    if (url.pathname.endsWith("/metrics")) {
      const now = Date.now();
      data = {
        latest: {
          capturedAt: now,
          cpuPercent: 12.5,
          memoryUsageBytes: 1048576,
          memoryLimitBytes: 10485760,
          memoryPercent: 10,
          networkRxBytesPerSecond: 1024,
          networkTxBytesPerSecond: 2048,
          writableLayerBytes: 4096,
          rootFsBytes: 1024000,
          restartCount: 4,
        },
        history: [
          {
            capturedAt: now - 5000,
            cpuPercent: 10,
            memoryUsageBytes: 1000000,
            networkRxBytesPerSecond: 512,
            networkTxBytesPerSecond: 1024,
          },
          {
            capturedAt: now,
            cpuPercent: 12.5,
            memoryUsageBytes: 1048576,
            networkRxBytesPerSecond: 1024,
            networkTxBytesPerSecond: 2048,
          },
        ],
        alerts: [
          {
            code: "FREQUENT_RESTART",
            message: "最近 10 分钟检测到至少 3 次重启",
          },
        ],
        intervalSeconds: 5,
        retentionHours: 24,
      };
    }
    return r.fulfill({ headers, json: { data, message: "" } });
  });
  return paths;
}
test("日志时间查询、搜索、输出类型、下载和实时连接", async ({ page }) => {
  const paths = await setup(page);
  let pushLog: ((text: string) => void) | undefined;
  await page.routeWebSocket(
    "**/api/ops/local/containers/*/logs/live?*",
    (ws) => {
      pushLog = (text) =>
        ws.send(
          JSON.stringify({
            type: "entry",
            entry: {
              stream: "stderr",
              timestamp: new Date().toISOString(),
              text,
            },
          }),
        );
      ws.send(JSON.stringify({ type: "connected", tty: false }));
      ws.send(
        JSON.stringify({
          type: "entry",
          entry: {
            stream: "stderr",
            timestamp: "2026-11-10T00:00:02Z",
            text: "live diagnostic error",
          },
        }),
      );
    },
  );
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
  await page.getByRole("menuitem", { name: "容器日志", exact: true }).click();
  const d = page.getByRole("dialog", {
    name: "容器详情 · test-customer",
    exact: true,
  });
  const content = d.getByLabel("日志内容");
  await expect(content).toContainText("service ready");
  await expect(content).toContainText("[stderr]");
  await expect(d).toContainText("实时日志已连接");
  await expect(
    d.getByRole("button", { name: "实时日志", exact: true }),
  ).toHaveCount(0);
  await d.getByRole("button", { name: "停止接收", exact: true }).hover();
  await expect(
    page.getByRole("tooltip", { name: "停止实时接收" }),
  ).toBeVisible();
  await expect(content).toHaveCSS("background-color", "rgb(17, 24, 39)");
  expect((await content.boundingBox())!.height).toBeGreaterThan(350);
  pushLog?.("automatic live update");
  await expect(content).toContainText("automatic live update");
  await d.getByLabel("日志关键词").fill("failed");
  await expect(content).not.toContainText("service ready");
  await expect(content).toContainText("request failed");
  await d.getByLabel("日志关键词").fill("");
  await d.getByRole("button", { name: /日志输出类型/ }).click();
  await page.getByRole("option", { name: "错误输出", exact: true }).click();
  const startDate = d
    .locator('[data-slot="date-picker"]')
    .filter({ has: page.getByText("日志开始时间", { exact: true }) });
  for (const [name, value] of [
    [/^year,/i, "2026"],
    [/^month,/i, "11"],
    [/^day,/i, "10"],
    [/^hour,/i, "08"],
    [/^minute,/i, "00"],
    [/^second,/i, "00"],
  ] as const) {
    await startDate.getByRole("spinbutton", { name }).fill(value);
  }
  await d.getByRole("button", { name: "选择日志开始时间" }).click();
  await expect(page.getByRole("grid", { name: /2026/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await d.getByRole("button", { name: "查询日志", exact: true }).click();
  await expect
    .poll(() =>
      paths.some((p) => p.includes("stream=stderr") && p.includes("since=")),
    )
    .toBeTruthy();
  await expect(content).not.toContainText("service ready");
  const download = page.waitForEvent("download");
  await d.getByRole("button", { name: "下载日志", exact: true }).click();
  expect((await download).suggestedFilename()).toMatch(/container-.*\.log/);
  await d.getByText("暂停滚动", { exact: true }).click();
  await d.getByRole("button", { name: "实时日志", exact: true }).click();
  await expect(content).toContainText("live diagnostic error");
  await expect(d.getByLabel("暂停滚动", { exact: true })).toBeChecked();
  await d.getByRole("button", { name: "停止接收", exact: true }).click();
  await expect(d).toContainText("已停止实时接收");
  await expect(
    d.getByRole("button", { name: "实时日志", exact: true }),
  ).toBeVisible();
  await expect(
    d.getByRole("button", { name: "停止接收", exact: true }),
  ).toHaveCount(0);
});
test("资源数值与趋势、异常提示及终端连接输入缩放断开", async ({ page }) => {
  await setup(page);
  const messages: unknown[] = [];
  await page.routeWebSocket("**/api/ops/local/containers/*/terminal", (ws) => {
    ws.send(
      JSON.stringify({
        type: "connected",
        sessionId: "test",
        idleSeconds: 600,
      }),
    );
    ws.onMessage((data) => {
      const message = JSON.parse(String(data));
      messages.push(message);
      if (message.type === "input") ws.send(Buffer.from("terminal ready\r\n"));
      if (message.type === "close")
        ws.send(JSON.stringify({ type: "closed", reason: "用户断开连接" }));
    });
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
    .getByRole("menuitem", { name: "容器详情与启停", exact: true })
    .click();
  const d = page.getByRole("dialog", {
    name: "容器详情 · test-customer",
    exact: true,
  });
  await expect(
    d.getByRole("tab", { name: "基础信息", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(d).toContainText("容器 ID");
  await expect(d.getByRole("tab")).toHaveText([
    "基础信息",
    "资源监控",
    "容器日志",
    "终端交互",
  ]);
  await expect(
    d.getByRole("button", { name: "版本与恢复点", exact: true }),
  ).toHaveCount(0);
  await expect(d.getByRole("region", { name: "实例资源监控" })).toHaveCount(0);
  await d.getByRole("tab", { name: "资源监控", exact: true }).click();
  await expect(d.getByText("容器 ID", { exact: true })).toHaveCount(0);
  await expect(d).toContainText("12.50%");
  await d.getByText("每 5 秒刷新指标", { exact: true }).click();
  await expect(
    d.getByRole("checkbox", { name: "每 5 秒刷新指标" }),
  ).not.toBeChecked();
  await expect(d).toContainText("至少 3 次重启");
  await expect(
    d.getByRole("img", { name: "CPU 使用率趋势", exact: true }),
  ).toBeVisible();
  await d.getByRole("tab", { name: "终端交互", exact: true }).click();
  await expect(
    d.getByRole("button", { name: "断开终端", exact: true }),
  ).toHaveCount(0);
  await d.getByRole("button", { name: "连接终端", exact: true }).hover();
  await expect(
    page.getByRole("tooltip", { name: "连接终端", exact: true }),
  ).toBeVisible();
  expect(
    (await d.getByLabel("终端输入输出").boundingBox())!.height,
  ).toBeGreaterThan(350);
  await expect(d.getByText("暂无连接记录", { exact: true })).not.toBeVisible();
  await d.getByRole("button", { name: "连接终端", exact: true }).click();
  await expect(d).toContainText("已连接");
  await expect(
    d.getByRole("button", { name: "连接终端", exact: true }),
  ).toHaveCount(0);
  await expect
    .poll(() =>
      messages.some(
        (v: unknown) =>
          !!v &&
          typeof v === "object" &&
          "type" in v &&
          v.type === "resize" &&
          "cols" in v &&
          typeof v.cols === "number" &&
          v.cols >= 20 &&
          "rows" in v &&
          typeof v.rows === "number" &&
          v.rows >= 5,
      ),
    )
    .toBeTruthy();
  const input = d.locator(".xterm-helper-textarea");
  await input.focus();
  await page.keyboard.type("echo test");
  await page.keyboard.press("Enter");
  await expect
    .poll(() =>
      messages.some(
        (v: unknown) =>
          !!v && typeof v === "object" && "type" in v && v.type === "input",
      ),
    )
    .toBeTruthy();
  await d.getByRole("button", { name: "断开终端", exact: true }).click();
  await expect(d).toContainText("已断开");
  await expect(
    d.getByRole("button", { name: "连接终端", exact: true }),
  ).toBeVisible();
  await d
    .getByRole("button", { name: "终端连接记录（0）", exact: true })
    .click();
  await expect(d.getByText("暂无连接记录", { exact: true })).toBeVisible();
  await d
    .getByRole("button", { name: "终端连接记录（0）", exact: true })
    .click();
  for (const viewport of [
    { width: 1440, height: 1000 },
    { width: 390, height: 844 },
  ]) {
    await page.setViewportSize(viewport);
    await d.getByRole("tab", { name: "基础信息", exact: true }).click();
    const dimensions = await d.boundingBox();
    expect(dimensions).not.toBeNull();
    if (!dimensions) throw new Error("容器详情不可见");
    expect(dimensions.width).toBeLessThanOrEqual(viewport.width - 32);
    expect(dimensions.height).toBeLessThanOrEqual(viewport.height - 32);
    if (viewport.width === 1440) {
      expect(dimensions.width).toBe(1000);
      expect(dimensions.height).toBe(760);
    }
    for (const name of ["资源监控", "容器日志", "终端交互", "基础信息"]) {
      await d.getByRole("tab", { name, exact: true }).click();
      const bounds = await d.boundingBox();
      expect(bounds?.width).toBe(dimensions.width);
      expect(bounds?.height).toBe(dimensions.height);
      expect(bounds?.y).toBe(dimensions.y);
    }
    const panel = d.getByRole("tabpanel", { name: "基础信息", exact: true });
    const scrollable = await panel.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
      return {
        height: el.clientHeight,
        content: el.scrollHeight,
        top: el.scrollTop,
      };
    });
    expect(scrollable.height).toBeGreaterThan(0);
    if (scrollable.content > scrollable.height)
      expect(scrollable.top).toBeGreaterThan(0);
    await expect(
      d.getByRole("tab", { name: "基础信息", exact: true }),
    ).toBeVisible();
  }
});

test("服务器已有普通容器无需客户关联即可进入终端", async ({ page }) => {
  await setup(page);
  const external = {
    id,
    name: "external-web",
    image: "nginx:latest",
    state: "running",
    association: null,
    protected: false,
    terminalAllowed: true,
    allowedActions: ["stop", "restart", "kill", "pause"],
    ports: [],
    mounts: [],
    addresses: [],
  };
  await page.route("**:8779/api/ops/local/containers", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          items: [external],
          total: 1,
          limit: 200,
          capturedAtMs: Date.now(),
        },
      },
    }),
  );
  await page.route(`**:8779/api/ops/local/containers/${id}`, (r) =>
    r.fulfill({ headers, json: { data: external } }),
  );
  await page.routeWebSocket("**/api/ops/local/containers/*/terminal", (ws) => {
    ws.send(
      JSON.stringify({
        type: "connected",
        sessionId: "external-test",
        idleSeconds: 600,
      }),
    );
  });
  await page.route("**:8779/api/ops/local/inventory", (r) =>
    r.fulfill({
      headers,
      json: {
        data: {
          info: {
            operatingSystem: "Linux",
            serverVersion: "26.1",
            cpus: 2,
            memory: 1024 ** 3,
          },
          images: [],
          networks: [],
          volumes: [],
          disk: [],
        },
      },
    }),
  );
  await page.goto("/ops/containers/containers");
  await page
    .getByRole("button", { name: "详情 external-web", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "容器详情 · external-web",
    exact: true,
  });
  await expect(
    dialog.getByRole("tab", { name: "终端交互", exact: true }),
  ).toBeEnabled();
  await dialog.getByRole("tab", { name: "终端交互", exact: true }).click();
  await dialog.getByRole("button", { name: "连接终端", exact: true }).click();
  await expect(dialog.getByText("已连接", { exact: true })).toBeVisible();
});
