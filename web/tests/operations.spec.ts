import { expect, test, type Page } from "@playwright/test";
import { portUrl } from "../app/components/ops/docker-types";
import { mockApi } from "./fixtures";

const nodeId = "node_00000000000000000000000000000001";
async function mockOps(
  page: Page,
  options: {
    disabled?: boolean;
    unsupported?: boolean;
    denied?: boolean;
    offline?: boolean;
    stale?: boolean;
    gpuMissing?: boolean;
  } = {},
) {
  await mockApi(page);
  await page.route("**:8779/api/session", (route) =>
    route.fulfill({
      json: {
        message: "",
        data: {
          user: {
            userId: "ops-admin",
            username: "admin",
            displayName: "管理员",
            role: options.denied ? "platform_finance" : "platform_admin",
            organizationId: "platform",
            status: "active",
          },
        },
      },
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
    }),
  );
  const calls: string[] = [];
  await page.route("**:8779/api/ops/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    calls.push(path);
    const capability = { available: true, reason: null };
    const node = {
      nodeId,
      name: "gpu-node-01",
      bootId: "boot-01",
      agentVersion: "0.1.0",
      protocolVersion: 1,
      docker: capability,
      hostMetrics: capability,
      nvidia: capability,
      status: "online",
      lastSeenAtMs: Date.now(),
    };
    let data: unknown;
    if (path === "/api/ops/context")
      data = {
        environmentSupported: !options.unsupported,
        enabled: !options.disabled,
        allowed: !options.denied,
        actions: options.denied
          ? []
          : ["nodes.read", "containers.read", "metrics.read"],
        writesEnabled: false,
        hostTerminalEnabled: false,
        reason: options.denied
          ? "当前阶段仅平台超级管理员可访问运维工作区"
          : options.disabled
            ? "当前环境尚未启用资源监控"
            : null,
      };
    else if (path === "/api/ops/nodes")
      data = {
        items: options.offline ? [] : [node],
        connectionError: options.offline
          ? "本机资源监控服务暂不可用，请稍后重试"
          : null,
        connectionErrorCode: options.offline ? "OPS_NODE_OFFLINE" : null,
      };
    else if (path.endsWith("/metrics/latest"))
      data = {
        nodeId,
        bootId: "boot-01",
        sequence: 1,
        capturedAtMs: Date.now() - (options.stale ? 60_000 : 0),
        host: {
          cpuPercent: 12.5,
          logicalCpus: 8,
          memoryUsedBytes: 4 * 1024 ** 3,
          memoryTotalBytes: 16 * 1024 ** 3,
          uptimeSeconds: 7200,
        },
        hostCapability: capability,
        gpuCapability: options.gpuMissing
          ? { available: false, reason: "driver_unavailable" }
          : capability,
        gpus: options.gpuMissing
          ? []
          : [
              {
                uuid: "GPU-test-01",
                name: "NVIDIA 测试 GPU",
                vendor: "nvidia",
                utilizationPercent: 40,
                memoryUsedBytes: 2 * 1024 ** 3,
                memoryTotalBytes: 8 * 1024 ** 3,
                temperatureCelsius: 60,
                powerWatts: null,
              },
            ],
      };
    else if (path.endsWith("/inventory"))
      data = {
        info: {
          serverVersion: "29.9.2",
          operatingSystem: "Docker Desktop",
          architecture: "x86_64",
          cpus: 8,
          memory: 8 * 1024 ** 3,
        },
        images: [
          {
            id: "image1",
            repository: "test-image",
            tag: "latest",
            size: "100MB",
            created: "1 hour ago",
          },
        ],
        networks: [
          { id: "network1", name: "bridge", driver: "bridge", scope: "local" },
        ],
        volumes: [{ name: "test-volume", driver: "local", scope: "local" }],
        disk: [
          { Type: "Images", Size: "100MB", Reclaimable: "0B", TotalCount: "1" },
        ],
        diskError: null,
      };
    else if (path.endsWith("/containers"))
      data = {
        items: [
          {
            id: "a".repeat(64),
            name: "yaya-api",
            image: "yaya-api:0.1.5",
            state: "running",
            terminalAllowed: true,
            allowedActions: ["stop", "restart", "kill", "pause"],
            protected: false,
            cpuPercent: 1.23,
            memoryPercent: 5.52,
            memoryUsage: "50MiB / 1GiB",
            addresses: [{ network: "bridge", address: "172.17.0.2" }],
            ports: [
              { containerPort: "80/tcp", hostIp: "0.0.0.0", hostPort: "8080" },
            ],
            project: "test-project",
            service: "api",
            createdAtMs: Date.now(),
          },
          {
            id: "b".repeat(64),
            name: "postgres",
            image: "postgres:17",
            state: "exited",
            terminalAllowed: false,
            allowedActions: ["start"],
            protected: false,
            cpuPercent: null,
            memoryPercent: null,
            memoryUsage: null,
            addresses: [],
            ports: [],
            project: null,
            service: null,
            createdAtMs: Date.now(),
          },
        ],
        limit: 200,
        truncated: false,
        capturedAtMs: Date.now(),
      };
    else throw new Error(`Unexpected operations endpoint ${path}`);
    await route.fulfill({
      json: { message: "", data },
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
    });
  });
  return calls;
}

test("统一导航展示运维与业务分组，跳转后保留展开状态", async ({ page }) => {
  await mockOps(page);
  await page.goto("/orders");
  const nav = page.getByRole("navigation", { name: "主导航" });
  await nav.locator("summary").filter({ hasText: "业务运营" }).click();
  await expect(nav.getByRole("link", { name: "订单中心" })).toBeVisible();
  await expect(page.getByLabel("工作区切换")).toHaveCount(0);
  await expect(nav.getByRole("link", { name: "GPU 监控" })).toBeHidden();
  await expect(
    nav.getByRole("link", { name: "日志系统", exact: true }),
  ).toHaveAttribute("href", "/logs");
  const platform = nav.locator("details").filter({ hasText: "平台管理" });
  await platform.locator("summary").click();
  await expect(platform.getByRole("link").first()).toHaveText("用户管理");
  await expect(nav.locator("details a[href='/logs']")).toHaveCount(0);
  await nav.locator("summary").filter({ hasText: "系统" }).click();
  await nav.getByRole("link", { name: "运维总览", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "运维总览", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("12.5%", { exact: true })).toBeVisible();
  await expect(page.getByLabel("当前节点")).toHaveValue(nodeId);
  await expect(nav.getByRole("link", { name: "GPU 监控" })).toBeVisible();
  await page.screenshot({
    path: "test-results/ops-overview.png",
    fullPage: true,
  });
  await nav.getByRole("link", { name: "订单中心" }).click();
  await expect(nav.getByRole("link", { name: "订单中心" })).toBeVisible();
  await nav.getByRole("link", { name: "运维总览", exact: true }).click();
  await expect(nav.getByRole("link", { name: "GPU 监控" })).toBeVisible();
});

test("默认概述、独立资源标签及容器详情列和状态操作", async ({ page }) => {
  const calls = await mockOps(page, { unsupported: true, disabled: true });
  await page.goto("/ops/containers");
  await expect(
    page.getByRole("tab", { name: "概述", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  expect(calls.sort()).toEqual(
    ["/api/ops/local/containers", "/api/ops/local/inventory"].sort(),
  );
  await page.getByRole("tab", { name: "镜像", exact: true }).click();
  await expect(page.getByRole("grid", { name: "本地镜像" })).toContainText(
    "test-image",
  );
  await page.getByRole("tab", { name: "网络", exact: true }).click();
  await expect(page.getByRole("grid", { name: "Docker 网络" })).toContainText(
    "bridge",
  );
  await page.getByRole("tab", { name: "存储卷", exact: true }).click();
  await expect(page.getByRole("grid", { name: "Docker 存储卷" })).toContainText(
    "test-volume",
  );
  await page.getByRole("tab", { name: "容器", exact: true }).click();
  await expect(page.getByText("yaya-api", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "终端 yaya-api", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "日志 yaya-api", exact: true }),
  ).toHaveCount(0);
  const more = page.getByRole("button", {
    name: "更多操作 yaya-api",
    exact: true,
  });
  await more.click();
  const menu = page.getByRole("menu", {
    name: "更多操作 yaya-api",
    exact: true,
  });
  await expect(menu).toBeVisible();
  for (const label of ["编辑", "升级", "备份", "制作镜像", "删除"]) {
    await expect(
      menu.getByRole("menuitem", { name: new RegExp(label) }),
    ).toBeDisabled();
  }
  await expect(menu.getByRole("menuitem")).toHaveCount(5);
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();

  await expect(page.getByRole("grid", { name: "Docker 容器" })).toContainText(
    "1.23%",
  );
  await expect(page.getByRole("grid", { name: "Docker 容器" })).toContainText(
    "172.17.0.2",
  );
  await expect(page.getByRole("link", { name: /8080/ })).toHaveAttribute(
    "href",
    "http://127.0.0.1:8080",
  );
  await page.getByRole("button", { name: "状态操作 yaya-api" }).hover();
  await expect(
    page.getByRole("menuitem", { name: "停止", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "状态操作 yaya-api" }).click();
  await expect(
    page.getByRole("menuitem", { name: "启动", exact: true }),
  ).toBeDisabled();
  await page.getByRole("menuitem", { name: "停止", exact: true }).click();
  await expect(
    page.getByRole("dialog", { name: "停止容器", exact: true }),
  ).toContainText("确认停止容器");
  await page.getByRole("button", { name: "取消", exact: true }).click();
  await page.getByLabel("搜索容器").fill("postgres");
  await expect(page.getByText("postgres", { exact: true })).toBeVisible();
  await expect(page.getByText("yaya-api", { exact: true })).toBeHidden();
});

test("容器状态单选筛选与搜索位于标题行右侧", async ({ page }) => {
  await mockOps(page);
  await page.goto("/ops/containers/containers");
  const filters = page.getByRole("radiogroup", { name: "容器状态筛选" });
  const running = filters.getByRole("radio", { name: /已启动/ });
  const all = filters.getByRole("radio", { name: /所有/ });
  await expect(all).toHaveAttribute("aria-checked", "true");
  await running.click();
  await expect(running).toHaveAttribute("aria-checked", "true");
  await expect(all).toHaveAttribute("aria-checked", "false");
  await expect(page).toHaveURL(/state=running/);
  const grid = page.getByRole("grid", { name: "Docker 容器", exact: true });
  await expect(grid).toContainText("yaya-api");
  await expect(grid).not.toContainText("postgres");
  await running.click();
  await expect(running).toHaveAttribute("aria-checked", "true");
  const title = page.getByRole("heading", { name: "Docker 容器", exact: true });
  const titleBox = await title.boundingBox();
  const filterBox = await filters.boundingBox();
  const searchBox = await page.getByLabel("搜索容器").boundingBox();
  expect(filterBox!.x).toBeGreaterThan(titleBox!.x + titleBox!.width);
  expect(Math.abs(filterBox!.y - titleBox!.y)).toBeLessThan(35);
  expect(searchBox!.x).toBeGreaterThan(filterBox!.x);
  await page.getByLabel("搜索容器").fill("missing-container");
  await expect(grid).not.toContainText("yaya-api");
  await page.reload();
  await expect(running).toHaveAttribute("aria-checked", "true");
  await expect(page.getByLabel("搜索容器")).toHaveValue("missing-container");
});

test("容器操作结果使用全局通知且不插入表格页面", async ({ page }) => {
  await mockOps(page);
  let failed = false;
  const headers = {
    "access-control-allow-origin": "http://127.0.0.1:8780",
    "access-control-allow-credentials": "true",
    "access-control-allow-methods": "GET,POST,OPTIONS",
    "access-control-allow-headers": "content-type",
  };
  await page.route("**/api/ops/local/containers/*/actions", (route) =>
    route.fulfill({ headers, json: { data: { taskId: "test-action" } } }),
  );
  await page.route("**/api/ops/local/actions/test-action", (route) =>
    route.fulfill({
      headers,
      json: {
        data: {
          status: failed ? "failed" : "succeeded",
          error: failed ? "Docker 操作测试失败" : null,
        },
      },
    }),
  );
  await page.goto("/ops/containers/containers");
  for (const fail of [false, true]) {
    failed = fail;
    await page.getByRole("button", { name: "状态操作 yaya-api" }).click();
    await page.getByRole("menuitem", { name: "停止", exact: true }).click();
    await page.getByRole("button", { name: "确认停止", exact: true }).click();
    const message = fail
      ? "Docker 操作测试失败"
      : "容器操作成功，已刷新实际状态。";
    await expect(page.getByText(message, { exact: true })).toBeVisible();
    await expect(page.locator("main")).not.toContainText(message);
  }
});

test("容器列表读取错误使用全局通知，重复失败不重复弹出", async ({ page }) => {
  await mockOps(page);
  let failures = 0;
  await page.route("**/api/ops/local/containers", (route) => {
    failures += 1;
    return route.fulfill({
      status: 503,
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: { data: null, message: "容器读取测试失败" },
    });
  });
  await page.goto("/ops/containers/containers");
  await expect(page.locator('[data-slot="toast"]')).toHaveCount(1);
  await expect(page.locator('[data-slot="toast"]')).toContainText(
    "容器读取测试失败",
  );
  await expect(page.locator("main")).not.toContainText("容器读取测试失败");
  await page.locator('button[aria-label="刷新当前页面"]').click();
  await expect.poll(() => failures).toBeGreaterThan(1);
  await expect(page.locator('button[aria-label="刷新当前页面"]')).toBeEnabled();
  await expect(page.locator('[data-slot="toast"]')).toHaveCount(1);
});

test("GPU 不支持的指标保持不可用，过期样本有明确提示", async ({ page }) => {
  await mockOps(page, { stale: true });
  await page.goto("/ops/gpu");
  await expect(
    page.getByRole("heading", { name: "NVIDIA 测试 GPU" }),
  ).toBeVisible();
  await expect(page.getByText("不可用", { exact: true })).toBeVisible();
  await expect(
    page.getByText("数据已过期，以下为最近一次成功采样。"),
  ).toBeVisible();
});

test("Docker 资源路由支持直达、搜索持久化及返回恢复", async ({ page }) => {
  await mockOps(page);
  await page.goto("/ops/containers/containers?search=postgres");
  const containers = page.getByRole("tab", { name: "容器", exact: true });
  await expect(containers).toHaveAttribute("aria-selected", "true");
  await expect(page.getByLabel("搜索容器")).toHaveValue("postgres");
  await expect(
    page.getByRole("grid", { name: "Docker 容器", exact: true }),
  ).not.toContainText("yaya-api");
  await expect(
    page.getByRole("grid", { name: "Docker 容器", exact: true }),
  ).toContainText("postgres");
  await page.reload();
  await expect(page.getByLabel("搜索容器")).toHaveValue("postgres");
  await page.getByRole("tab", { name: "镜像", exact: true }).click();
  await expect(page).toHaveURL(/\/ops\/containers\/images\?search=postgres$/);
  await expect(page.getByRole("grid", { name: "本地镜像" })).toContainText(
    "test-image",
  );
  await page.goBack();
  await expect(containers).toHaveAttribute("aria-selected", "true");
  await expect(page.getByLabel("搜索容器")).toHaveValue("postgres");
  await page.getByLabel("搜索容器").fill("yaya-api");
  await expect(page).toHaveURL(/search=yaya-api$/);
  await expect(
    page.getByRole("grid", { name: "Docker 容器", exact: true }),
  ).not.toContainText("postgres");
  for (const [tab, label] of [
    ["overview", "概述"],
    ["compose", "编排"],
    ["images", "镜像"],
    ["networks", "网络"],
    ["volumes", "存储卷"],
    ["configuration", "配置"],
  ]) {
    await page.goto(`/ops/containers/${tab}`);
    await expect(
      page.getByRole("tab", { name: label, exact: true }),
    ).toHaveAttribute("aria-selected", "true");
  }
});

test("驱动异常明确显示原因", async ({ page }) => {
  await mockOps(page, { gpuMissing: true });
  await page.goto("/ops/gpu");
  await expect(
    page.getByText("NVIDIA 驱动不可用", { exact: true }),
  ).toBeVisible();
});

test("监控服务不可用时显示明确原因，不伪造资源数据", async ({ page }) => {
  const calls = await mockOps(page, { offline: true });
  await page.goto("/ops/overview");
  await expect(
    page.getByRole("heading", { name: "资源监控暂不可用" }),
  ).toBeVisible();
  await expect(
    page.getByRole("alert").filter({ hasText: "本机资源监控服务暂不可用" }),
  ).toBeVisible();
  expect(calls.some((path) => path.endsWith("/metrics/latest"))).toBe(false);
});

for (const denied of [false, true]) {
  test(`功能${denied ? "无权限" : "关闭"}时不请求节点资源`, async ({
    page,
  }) => {
    const calls = await mockOps(page, { disabled: !denied, denied });
    await page.goto("/ops/overview");
    await expect(page.getByRole("heading", { name: "资源总览" })).toBeVisible();
    await expect(page.getByRole("status")).toContainText(
      denied ? "没有资源监控访问权限" : "未启用",
    );
    expect(calls).toEqual(["/api/ops/context"]);
    if (denied)
      await expect(
        page.getByRole("link", { name: "运维", exact: true }),
      ).toHaveCount(0);
  });
}

test("手机宽度的运维页面不撑破视口", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockOps(page);
  await page.goto("/ops/gpu");
  await expect(
    page.getByRole("heading", { name: "NVIDIA 测试 GPU" }),
  ).toBeVisible();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
});

for (const [route, resource] of [
  ["overview", "资源总览"],
  ["nodes", "主机节点信息"],
  ["gpu", "GPU 监控"],
]) {
  test(`不支持的系统显示${resource}环境限制`, async ({ page }) => {
    const calls = await mockOps(page, { unsupported: true, disabled: true });
    await page.goto(`/ops/${route}`);
    await expect(page.getByRole("status")).toContainText(
      `当前系统环境不支持查看${resource}`,
    );
    await expect(page.getByRole("status")).toContainText("Linux 宿主机环境");
    await expect(page.locator("main")).not.toContainText("Agent");
    expect(calls).toEqual(["/api/ops/context"]);
  });
}

test("容器页面拒绝无权限账号且不请求 Docker", async ({ page }) => {
  const calls = await mockOps(page, { denied: true });
  await page.goto("/ops/containers");
  await expect(
    page.getByRole("alert").filter({ hasText: "没有容器管理访问权限" }),
  ).toBeVisible();
  expect(calls).toEqual([]);
});
test("Docker 断连显示异常，恢复后可刷新容器列表", async ({ page }) => {
  await mockOps(page, { disabled: true, unsupported: true });
  let failed = true;
  await page.route("**:8779/api/ops/local/containers", async (route) => {
    await route.fulfill({
      status: failed ? 503 : 200,
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: failed
        ? {
            message: "无法连接本机 Docker，请确认 Docker Desktop 已启动",
            data: null,
          }
        : {
            message: "",
            data: {
              items: [],
              limit: 200,
              truncated: false,
              capturedAtMs: Date.now(),
            },
          },
    });
  });
  await page.goto("/ops/containers");
  await expect(
    page.getByRole("alert").filter({ hasText: "Docker Desktop" }),
  ).toBeVisible();
  await page.getByRole("tab", { name: "容器", exact: true }).click();
  failed = false;
  await page.getByLabel("刷新当前页面", { exact: true }).click();
  await expect(
    page.getByText("没有符合条件的容器", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("alert").filter({ hasText: "Docker Desktop" }),
  ).toHaveCount(0);
});

test("端口访问正确处理通配地址、IPv6、UDP 和未发布端口", () => {
  expect(
    portUrl(
      { hostIp: "0.0.0.0", hostPort: "8080", containerPort: "80/tcp" },
      "panel.example.com",
    ),
  ).toBe("http://panel.example.com:8080");
  expect(
    portUrl(
      { hostIp: "::", hostPort: "8443", containerPort: "443/tcp" },
      "::1",
    ),
  ).toBe("https://[::1]:8443");
  expect(
    portUrl(
      { hostIp: "127.0.0.1", hostPort: "5353", containerPort: "53/udp" },
      "panel.example.com",
    ),
  ).toBeNull();
  expect(
    portUrl(
      { hostIp: null, hostPort: null, containerPort: "80/tcp" },
      "localhost",
    ),
  ).toBeNull();
});

test("概述和容器每五秒自动刷新，其他标签停止且可以关闭", async ({ page }) => {
  await page.clock.install({ time: new Date("2026-11-10T00:00:00Z") });
  await page.clock.pauseAt(new Date("2026-11-10T00:00:01Z"));
  const calls = await mockOps(page, { unsupported: true, disabled: true });
  const resourceCalls = () =>
    calls.filter((path) => path.startsWith("/api/ops/local/")).length;
  await page.goto("/ops/containers");
  const toggle = page.getByRole("checkbox", { name: "每 5 秒自动刷新" });
  await expect(toggle).toBeChecked();
  await expect.poll(resourceCalls).toBe(2);
  await page.clock.runFor(4999);
  expect(resourceCalls()).toBe(2);
  await page.clock.runFor(1);
  await expect.poll(resourceCalls).toBe(4);
  await page.getByRole("tab", { name: "容器", exact: true }).click();
  await expect(toggle).toBeChecked();
  await expect(
    page.getByRole("columnheader", { name: "客户 / 实例", exact: true }),
  ).toHaveCount(0);
  await page.clock.runFor(4999);
  expect(resourceCalls()).toBe(4);
  await page.clock.runFor(1);
  await expect.poll(resourceCalls).toBe(6);
  await page.getByRole("tab", { name: "镜像", exact: true }).click();
  await expect(toggle).toHaveCount(0);
  await page.clock.runFor(15000);
  expect(resourceCalls()).toBe(6);
  await page.getByRole("tab", { name: "容器", exact: true }).click();
  await expect(toggle).toBeChecked();
  await page.clock.runFor(5000);
  await expect.poll(resourceCalls).toBe(8);
  await toggle.uncheck();
  await page.clock.runFor(10000);
  expect(resourceCalls()).toBe(8);
});
