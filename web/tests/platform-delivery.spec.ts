import { expect, test, type Page } from "@playwright/test";
import { mockApi } from "./fixtures";
const pkg = "6bab0f03-9dac-414a-93d2-ce1f62f9d120";
async function setup(
  page: Page,
  {
    noDocker = false,
    denied = false,
    saveError = false,
    deployError = false,
    failedLoad = false,
  } = {},
) {
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
  const headers = {
    "access-control-allow-origin": "http://127.0.0.1:8780",
    "access-control-allow-credentials": "true",
  };
  await page.route("**:8779/api/session", (r) =>
    r.fulfill({
      headers,
      json: {
        message: "",
        data: {
          user: {
            userId: "admin",
            role: denied ? "platform_operator" : "platform_admin",
            username: "admin",
            displayName: "管理员",
          },
        },
      },
    }),
  );
  const writes: string[] = [];
  const instanceBodies: Record<string, unknown>[] = [];
  const registryRequests: string[] = [];
  let record: Record<string, unknown> | null = null;
  let createCount = 0;
  let registries = [
    {
      registryId: "dockerhub",
      alias: "DockerHub",
      url: "https://hub.docker.com",
      username: "",
      hasPassword: false,
      mirrors: [] as string[],
      isDefault: true,
    },
  ];
  await page.route(
    "**:8779/api/{platform-**,customers/c1/platform-instances}",
    async (route) => {
      const req = route.request();
      const rawPath = new URL(req.url()).pathname;
      const path =
        rawPath === "/api/customers/c1/platform-instances"
          ? "/api/platform-instances"
          : rawPath;
      const reply = (data: unknown, status = 200, message = "") =>
        route.fulfill({ headers, status, json: { message, data } });
      if (req.method() !== "GET") writes.push(`${req.method()} ${path}`);
      if (path.startsWith("/api/platform-registries")) {
        registryRequests.push(path);
        if (req.method() === "GET") return reply(registries);
        const id = path.split("/")[3];
        if (req.method() === "DELETE") {
          registries = registries.filter((r) => r.registryId !== id);
          return reply(true);
        }
        const body = req.postDataJSON();
        const old = registries.find((r) => r.registryId === id);
        if (body.isDefault)
          registries = registries.map((r) => ({ ...r, isDefault: false }));
        const saved = {
          registryId: id || "registry-new",
          alias: body.alias,
          url: body.url,
          username: body.username,
          hasPassword: !!body.password || !!old?.hasPassword,
          mirrors: body.mirrors,
          isDefault: body.isDefault,
        };
        registries = [
          ...registries.filter((r) => r.registryId !== saved.registryId),
          saved,
        ];
        return reply(saved);
      }
      if (path === "/api/platform-networks")
        return reply({
          items: [
            {
              name: "bridge",
              driver: "bridge",
              subnets: ["172.17.0.0/16"],
              internal: false,
            },
            {
              name: "customer-lan",
              driver: "macvlan",
              subnets: ["192.168.50.0/24"],
              internal: false,
            },
          ],
          hostSupported: false,
          hostReason: "当前离线镜像内置数据库，暂不支持 host 共享网络",
        });
      if (path === "/api/platform-runtime")
        return reply({
          available: !noDocker,
          message: noDocker
            ? "当前环境未安装 Docker CLI"
            : "当前环境 Docker 可用",
          serverVersion: noDocker ? null : "27",
        });
      if (path === "/api/platform-instances/options")
        return reply({
          customers: [{ id: "c1", name: "客户 A" }],
          packages: [
            {
              id: pkg,
              name: "1.13a",
              environment: [
                "ALIYUN_SMS_SIGN_NAME=",
                "ALIYUN_SMS_REGION=cn-hangzhou",
                "PATH=/usr/bin:/bin",
              ],
              deployment: {
                webPort: 8787,
                apiPort: 8788,
                volumes: [
                  { key: "database", destination: "/var/lib/postgresql" },
                  { key: "platform-state", destination: "/var/lib/yaya" },
                ],
                environment: [
                  { name: "APP_ENV", required: true, default: "production" },
                  { name: "POSTGRES_PASSWORD", required: true },
                  { name: "POSTGRES_DB", required: true },
                  { name: "POSTGRES_USER", required: true },
                  { name: "VALKEY_PASSWORD", required: true },
                  { name: "AUTH_TOKEN_SECRET", required: true },
                  {
                    name: "VALKEY_CACHE_TTL_HOURS",
                    required: true,
                    default: "8",
                  },
                  { name: "ALIYUN_SMS_REGION", default: "cn-hangzhou" },
                ],
              },
            },
            {
              id: "package-two",
              name: "2.0",
              environment: ["SECOND_PACKAGE_ONLY=from-image"],
            },
            { id: "legacy", name: "0.9" },
          ],
        });
      if (path === "/api/platform-instances" && req.method() === "GET")
        return reply(record ? [record] : []);
      if (path.endsWith("/deployments"))
        return reply([
          {
            deploymentId: "task1",
            status: failedLoad ? "failed" : "running",
            stage: "loading",
            logTail: "正在从离线包导入完整镜像",
            error: failedLoad
              ? "Docker image load 失败（退出码 1）：\ninvalid tar header: image archive cannot be loaded"
              : null,
            createdAt: Date.now(),
          },
        ]);
      if (path.endsWith("/deploy"))
        return deployError
          ? reply(null, 503, "Docker 已停止，请启动 Docker")
          : reply("task1");
      if (path.endsWith("/archive")) {
        record = { ...record, status: "archived" };
        return reply(record);
      }
      if (req.method() === "POST" || req.method() === "PUT") {
        if (saveError)
          return reply(null, 409, "当前 Docker 环境的端口已被其他平台实例占用");
        const body = req.postDataJSON();
        expect(body).not.toHaveProperty("nodeId");
        instanceBodies.push(body);
        if (req.method() === "POST") createCount++;
        record = {
          ...body,
          containerName: record?.containerName ?? `${body.name}-1`,
          instanceId: "instance1",
          customerName: "客户 A",
          version: "1.13a",
          status: "not_deployed",
          environmentKeys: [
            "APP_PORT",
            "POSTGRES_PASSWORD",
            ...String(body.environment ?? "")
              .split("\n")
              .filter(Boolean)
              .map((line) => line.split("=")[0]),
          ],
          containerId: null,
          imageId: null,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
        return reply(record);
      }
      return reply(null, 404);
    },
  );
  return {
    writes,
    instanceBodies,
    registryRequests,
    creates: () => createCount,
  };
}
async function fill(page: Page) {
  await page.getByRole("button", { name: "新建实例", exact: true }).click();
  const d = page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ });
  await expect(d.getByLabel("实例名称")).toBeDisabled();
  await expect(d.getByLabel("实例名称")).toHaveValue("");
  await expect(d.getByRole("button", { name: /所属客户/ })).toBeDisabled();
  await d.getByRole("button", { name: "平台版本包" }).click();
  await page
    .getByRole("option", { name: `1.13a · ${pkg}`, exact: true })
    .click();
  await expect(d.getByLabel("实例名称")).toHaveValue("lowcode-1.13a-c1");
  await expect(d.getByLabel("容器名称")).toBeDisabled();
  await d.getByLabel("平台域名").fill("https://customer.example.com");
  await d
    .getByRole("textbox", { name: "ALIYUN_SMS_SIGN_NAME 的值", exact: true })
    .fill("客户A");
  return d;
}

test("实例运维入口记录原因、展示审计并可撤销会话", async ({ page }) => {
  await setup(page);
  const headers = {
    "access-control-allow-origin": "http://127.0.0.1:8780",
    "access-control-allow-credentials": "true",
  };
  await page.route("**:8779/api/customers/c1/platform-instances", (route) =>
    route.fulfill({
      headers,
      json: {
        message: "",
        data: [
          {
            instanceId: "support",
            name: "客户运维平台",
            customerName: "客户 A",
            version: "1.14a",
            packageId: pkg,
            containerName: "support-container",
            containerId: "a".repeat(64),
            status: "running",
            licenseSyncStatus: "synced",
            runtime: { state: "running" },
          },
        ],
      },
    }),
  );
  const reasons: string[] = [];
  let revoked = false;
  await page.route(
    "**:8779/api/platform-instances/support/support-access**",
    async (route) => {
      if (route.request().method() === "OPTIONS")
        return route.fulfill({
          status: 204,
          headers: {
            ...headers,
            "access-control-allow-methods": "GET,POST,OPTIONS",
            "access-control-allow-headers": "content-type",
          },
        });
      if (route.request().url().endsWith("/revoke")) {
        revoked = true;
        return route.fulfill({
          headers,
          json: { message: "已撤销", data: {} },
        });
      }
      if (route.request().method() === "POST") {
        reasons.push(route.request().postDataJSON().reason);
        return route.fulfill({
          headers,
          json: {
            message: "已生成",
            data: {
              url: "http://127.0.0.1:18887/operations-login#code=temporary",
              expiresIn: 60,
            },
          },
        });
      }
      return route.fulfill({
        headers,
        json: {
          message: "",
          data: {
            sessions: [
              {
                id: "s",
                username: "admin",
                reason: "排查客户页面异常",
                createdAt: Date.now(),
                connectedAt: Date.now(),
                expiresAt: Date.now() + 60000,
                endedAt: null,
              },
            ],
            events: [
              {
                sessionId: "s",
                event: "request",
                method: "GET",
                path: "/api/apps",
                createdAt: Date.now(),
              },
            ],
          },
        },
      });
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
  const supportButton = page.getByRole("button", { name: "平台运维访问", exact: true });
  await supportButton.hover();
  await expect(page.getByRole("tooltip")).toHaveText("平台运维访问");
  await supportButton.click();
  const modal = page.getByRole("dialog", { name: "平台运维访问", exact: true });
  await expect(
    modal.getByRole("button", { name: "生成一次性登录入口" }),
  ).toBeDisabled();
  await modal.getByLabel("运维访问原因").fill("排查客户页面异常");
  await modal.getByRole("button", { name: "生成一次性登录入口" }).click();
  await expect(
    modal.getByRole("link", { name: "进入客户平台" }),
  ).toHaveAttribute("href", /operations-login#code=/);
  expect(reasons).toEqual(["排查客户页面异常"]);
  await expect(modal).toContainText("GET /api/apps");
  await modal.getByRole("button", { name: "撤销此实例全部运维会话" }).click();
  await expect(modal.getByRole("link", { name: "进入客户平台" })).toHaveCount(
    0,
  );
  expect(revoked).toBe(true);
});

test("Docker 断连仍展示已部署实例和真实未知状态", async ({ page }) => {
  await setup(page, { noDocker: true });
  await page.route("**:8779/api/customers/c1/platform-instances", (r) =>
    r.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        message: "",
        data: [
          {
            instanceId: "offline",
            name: "断连客户平台",
            customerName: "客户 A",
            customerId: "c1",
            version: "1.13a",
            packageId: pkg,
            containerName: "yaya-offline",
            containerId: "a".repeat(64),
            status: "running",
            runtime: {
              state: "unknown",
              checkedAt: Date.now(),
              message: "Docker 未连接",
            },
          },
        ],
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
  const grid = page.getByRole("list", { name: "平台实例", exact: true });
  await expect(grid).toContainText("断连客户平台");
  await expect(grid).toContainText("状态未知");
  await expect(grid).not.toContainText("运行中");
  await expect(
    page.getByRole("button", { name: "新建实例", exact: true }),
  ).toBeDisabled();
});

test("实例直达容器日志、停止任务及状态同步，展示操作记录", async ({ page }) => {
  await setup(page);
  const id = "a".repeat(64);
  let state = "running";
  let stopped = false;
  let releaseRefresh!: () => void;
  const refreshGate = new Promise<void>((resolve) => {
    releaseRefresh = resolve;
  });
  const headers = {
    "access-control-allow-origin": "http://127.0.0.1:8780",
    "access-control-allow-credentials": "true",
  };
  await page.route("**:8779/api/customers/c1/platform-instances", (r) =>
    r.fulfill({
      headers,
      json: {
        message: "",
        data: [
          {
            instanceId: "bound",
            name: "客户运行平台",
            customerName: "客户 A",
            version: "1.13a",
            packageId: pkg,
            containerName: "yaya-bound",
            containerId: id,
            status: "running",
            runtime: { state, checkedAt: Date.now() },
          },
        ],
      },
    }),
  );
  await page.route("**:8779/api/ops/local/**", async (r) => {
    const path = new URL(r.request().url()).pathname;
    const reply = (data: unknown) =>
      r.fulfill({ headers, json: { message: "", data } });
    if (path.endsWith("/logs")) return reply({ text: "2026-11-10 API ready" });
    if (path.endsWith("/actions") && r.request().method() === "POST") {
      expect(r.request().postDataJSON()).toEqual({
        action: "stop",
        expectedState: "running",
      });
      state = "exited";
      stopped = true;
      return reply({ taskId: "stop-task" });
    }
    if (path.endsWith("/actions"))
      return reply(
        stopped
          ? [
              {
                taskId: "stop-task",
                action: "stop",
                status: "succeeded",
                error: null,
                createdAtMs: Date.now(),
              },
            ]
          : [],
      );
    if (path.endsWith("/stop-task"))
      return reply({ taskId: "stop-task", status: "succeeded" });
    if (stopped) await refreshGate;
    return reply({
      id,
      name: "yaya-bound",
      image: "yaya:1.13a",
      state,
      createdAtMs: Date.now(),
      association: {
        instanceId: "bound",
        instanceName: "客户运行平台",
        customerName: "客户 A",
      },
      terminalAllowed: true,
      allowedActions: state === "running" ? ["stop", "restart"] : ["start"],
      ports: [],
      mounts: [{ name: "customer-data", destination: "/var/lib/yaya" }],
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
    .getByRole("button", { name: "更多操作 客户运行平台", exact: true })
    .click();
  await page.getByRole("menuitem", { name: "容器日志", exact: true }).click();
  const d = page.getByRole("dialog", {
    name: "容器详情 · yaya-bound",
    exact: true,
  });
  await expect(d).toContainText("2026-11-10 API ready");
  await d.getByRole("tab", { name: "基础信息", exact: true }).click();
  await expect(d).toContainText("customer-data");
  await d.getByRole("button", { name: "停止容器", exact: true }).click();
  const confirmation = page.getByRole("alertdialog", {
    name: "停止容器",
    exact: true,
  });
  await expect(confirmation).toContainText("保留全部数据卷");
  await confirmation
    .getByRole("button", { name: "取消操作", exact: true })
    .click();
  await expect(confirmation).toBeHidden();
  expect(stopped).toBe(false);
  await d.getByRole("button", { name: "停止容器", exact: true }).click();
  await confirmation
    .getByRole("button", { name: "确认停止", exact: true })
    .click();
  await expect(
    d.getByRole("button", { name: "停止中…", exact: true }),
  ).toBeDisabled();
  await expect(
    d.getByRole("button", { name: "刷新容器详情", exact: true }),
  ).toBeDisabled();
  releaseRefresh();
  await expect(
    d.getByRole("button", { name: "启动容器", exact: true }),
  ).toBeEnabled();
  await expect(
    d.getByRole("button", { name: "停止容器", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByText("操作完成，已核对容器实际状态。", { exact: true }),
  ).toBeVisible();
  await expect(d).not.toContainText("操作完成");
  await expect(d).toContainText("已停止");
  await expect(d).toContainText("已完成");
  await expect(
    page.getByRole("list", { name: "平台实例", exact: true }),
  ).toContainText("已停止");
});
test("创建本机实例填写域名容器版本环境变量并直接部署", async ({ page }) => {
  const { writes, registryRequests } = await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  await expect(d.getByLabel("目标主机")).toHaveCount(0);
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ }),
  ).toHaveCount(0);
  const grid = page.getByRole("list", { name: "平台实例", exact: true });
  await expect(grid).toContainText("lowcode-1.13a-c1-1");
  await expect(grid).toContainText("https://customer.example.com");
  expect(writes).toEqual([
    "POST /api/platform-instances",
    "POST /api/platform-instances/instance1/deploy",
  ]);
  expect(registryRequests).toEqual([]);
  await page
    .getByRole("button", { name: "更多操作 lowcode-1.13a-c1", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "部署记录 lowcode-1.13a-c1", exact: true })
    .click();
  await expect(
    page.getByRole("dialog", { name: "Docker 部署记录", exact: true }),
  ).toContainText("正在从离线包导入完整镜像");
});
test("没有 Docker 保留实例列表并禁止创建部署", async ({ page }) => {
  const { writes } = await setup(page, { noDocker: true });
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  await expect(
    page.getByRole("status").filter({ hasText: "当前环境未安装 Docker CLI" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "新建实例", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("list", { name: "平台实例", exact: true }),
  ).toHaveCount(1);
  expect(writes).toEqual([]);
});

test("新建实例名称禁用且根据版本与客户自动生成", async ({ page }) => {
  const { instanceBodies } = await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  const name = d.getByRole("textbox", { name: "实例名称", exact: true });
  await expect(name).toBeDisabled();
  await expect(name).not.toHaveAttribute("required", "");
  await d.getByRole("button", { name: "平台版本包" }).click();
  await page
    .getByRole("option", { name: "2.0 · package-two", exact: true })
    .click();
  await expect(name).toHaveValue("lowcode-2.0-c1");
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ }),
  ).toHaveCount(0);
  expect(instanceBodies[0].name).toBe("lowcode-2.0-c1");
  expect(instanceBodies[0]).not.toHaveProperty("containerName");
});

test("实例卡片展示状态、直接展示部署信息、搜索及窄屏操作", async ({ page }) => {
  await setup(page);
  const statuses = [
    "not_deployed",
    "deploying",
    "running",
    "failed",
    "archived",
  ];
  await page.route("**:8779/api/customers/c1/platform-instances", (route) =>
    route.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        message: "",
        data: statuses.map((status, index) => ({
          instanceId: `instance-${index}`,
          name: `客户平台 ${index}`,
          customerName: "长名称客户测试",
          customerId: "c1",
          version: "1.13a",
          packageId: pkg,
          containerName: `yaya-customer-${index}`,
          publicUrl: "https://customer.example.com",
          webPort: 8787,
          apiPort: 8788,
          status,
          containerId: status === "running" ? "container-id" : null,
          runtime:
            status === "running"
              ? {
                  state: "running",
                  checkedAt: Date.now(),
                  health: null,
                  exitCode: 0,
                  message: null,
                }
              : null,
          environmentKeys: [],
          notes: "",
          createdAt: 1,
          updatedAt: 1,
        })),
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
  const grid = page.getByRole("list", { name: "平台实例", exact: true });
  const row = grid.getByRole("listitem").filter({ hasText: "客户平台 0" });
  await expect(row).toContainText("版本 1.13a");
  await expect(
    row.getByRole("link", { name: "yaya-customer-0", exact: true }),
  ).toBeVisible();
  await expect(row).toContainText(pkg);
  await expect(
    row.getByRole("link", { name: "yaya-customer-0", exact: true }),
  ).toHaveAttribute(
    "href",
    "/ops/containers/containers?search=yaya-customer-0",
  );
  await expect(
    row.getByRole("link", {
      name: "https://customer.example.com",
      exact: true,
    }),
  ).toHaveAttribute("target", "_blank");
  await expect(row).toContainText("Web 8787 / API 8788");
  await expect(row.getByRole("button")).toHaveCount(2);
  for (const [text] of [
    ["待部署", "warning"],
    ["部署中", "accent"],
    ["运行中", "success"],
    ["部署失败", "danger"],
    ["已归档", "default"],
  ]) {
    await expect(
      grid.locator("[data-instance-status]").filter({ hasText: text }),
    ).toBeVisible();
  }
  const more = row.getByRole("button", {
    name: "更多操作 客户平台 0",
    exact: true,
  });
  await more.hover();
  await expect(
    page.getByRole("menu", { name: "更多操作 客户平台 0", exact: true }),
  ).toHaveCount(0);
  await more.click();
  await expect(
    page.getByRole("menu", { name: "更多操作 客户平台 0", exact: true }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(
    page.getByRole("menu", { name: "更多操作 客户平台 0", exact: true }),
  ).toHaveCount(0);
  await more.click();
  await expect(
    page.getByRole("menuitem", { name: "部署记录 客户平台 0", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("menuitem", { name: "编辑 客户平台 0", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("menuitem", { name: "归档 客户平台 0", exact: true })
    .click();
  await expect(
    page.getByRole("dialog", { name: "归档平台实例", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("dialog", { name: /^(?!.*客户业务).*$/ })
    .getByRole("button", { name: "关闭", exact: true })
    .click();
  await grid
    .getByRole("button", { name: "查看 客户平台 0", exact: true })
    .click();
  await expect(
    page.getByRole("dialog", { name: "平台实例详情", exact: true }),
  ).toContainText(pkg);
  await page
    .getByRole("dialog", { name: /^(?!.*客户业务).*$/ })
    .getByRole("button", { name: "关闭", exact: true })
    .click();
  await page.setViewportSize({ width: 1520, height: 900 });
  await page.screenshot({
    path: "test-results/platform-instances-cards-desktop.png",
    fullPage: true,
    animations: "disabled",
  });
  await page.getByRole("textbox", { name: "搜索平台实例" }).fill(pkg);
  await expect(grid.getByRole("listitem")).toHaveCount(5);
  await page.getByRole("textbox", { name: "搜索平台实例" }).fill("客户平台 0");
  await expect(grid.getByRole("listitem")).toHaveCount(1);
  await page
    .getByRole("textbox", { name: "搜索平台实例" })
    .fill("不存在的实例");
  await expect(
    page.getByText("没有匹配的实例。", { exact: true }),
  ).toBeVisible();
  await page.getByRole("textbox", { name: "搜索平台实例" }).fill("");
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await grid.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(
    true,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/platform-instances-cards-mobile.png",
    fullPage: true,
    animations: "disabled",
  });
});

test("先选版本包才显示镜像环境变量，切换包重新生成，旧包不套用模板", async ({
  page,
}) => {
  await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  await page.getByRole("button", { name: "新建实例", exact: true }).click();
  const d = page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ });
  await expect(
    d.getByRole("region", { name: "平台环境变量", exact: true }),
  ).toHaveCount(0);
  const choose = async (name: string) => {
    await d.getByRole("button", { name: "平台版本包" }).click();
    await page.getByRole("option", { name, exact: true }).click();
  };
  await choose(`1.13a · ${pkg}`);
  await expect(
    d.getByRole("textbox", { name: "APP_ENV 的值", exact: true }),
  ).toHaveValue("production");
  await expect(
    d.getByRole("textbox", { name: "APP_ENV 的值", exact: true }),
  ).toBeDisabled();
  await expect(
    d.getByRole("textbox", { name: "POSTGRES_PASSWORD 的值", exact: true }),
  ).toHaveValue("");
  for (const name of [
    "POSTGRES_PASSWORD",
    "VALKEY_PASSWORD",
    "AUTH_TOKEN_SECRET",
  ]) {
    const input = d.getByRole("textbox", { name: `${name} 的值`, exact: true });
    await expect(input).toHaveValue("");
    await expect(input).toHaveAttribute("placeholder", /创建时由平台自动生成/);
    await expect(input).toBeDisabled();
  }
  await expect(
    d.getByRole("textbox", { name: "POSTGRES_DB 的值", exact: true }),
  ).toHaveValue("yaya_low_code");
  await expect(
    d.getByRole("textbox", { name: "POSTGRES_USER 的值", exact: true }),
  ).toHaveValue("yaya");
  await expect(
    d.getByRole("textbox", { name: "POSTGRES_DB 的值", exact: true }),
  ).toHaveAttribute("placeholder", /平台 PostgreSQL 数据库名称/);
  await expect(
    d.getByRole("textbox", { name: "POSTGRES_DB 的值", exact: true }),
  ).toBeDisabled();
  await expect(
    d.getByRole("textbox", { name: "POSTGRES_USER 的值", exact: true }),
  ).toBeDisabled();
  await expect(
    d.getByRole("textbox", {
      name: "VALKEY_CACHE_TTL_HOURS 的值",
      exact: true,
    }),
  ).toHaveValue("8");
  await expect(
    d.getByRole("textbox", { name: "ALIYUN_SMS_REGION 的值", exact: true }),
  ).toHaveCount(1);
  await expect(
    d.getByRole("textbox", { name: "ALIYUN_SMS_REGION 的值", exact: true }),
  ).toHaveValue("cn-hangzhou");
  await expect(
    d.getByRole("textbox", { name: "PATH 的值", exact: true }),
  ).toBeDisabled();
  await d
    .getByRole("textbox", { name: "ALIYUN_SMS_REGION 的值", exact: true })
    .fill("changed");
  await choose("2.0 · package-two");
  await expect(
    d.getByRole("textbox", { name: "ALIYUN_SMS_REGION 的值", exact: true }),
  ).toHaveCount(0);
  await expect(
    d.getByRole("textbox", { name: "SECOND_PACKAGE_ONLY 的值", exact: true }),
  ).toHaveValue("from-image");
  await choose(`1.13a · ${pkg}`);
  await expect(
    d.getByRole("textbox", { name: "ALIYUN_SMS_REGION 的值", exact: true }),
  ).toHaveValue("cn-hangzhou");
  await choose("0.9 · legacy");
  await expect(d.getByRole("status")).toContainText("未携带镜像环境变量快照");
  await expect(d.getByRole("group", { name: /^环境变量 \d+$/ })).toHaveCount(0);
});

test("环境变量支持添加移除，值中的等号原样保存，编辑留空保留原值", async ({
  page,
}) => {
  const { instanceBodies } = await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  await expect(
    d.getByRole("textbox", { name: "ALIYUN_SMS_REGION 的值", exact: true }),
  ).toHaveValue("cn-hangzhou");
  await d.getByRole("button", { name: "添加环境变量", exact: true }).click();
  const added = d.getByRole("group", { name: /^环境变量 \d+$/ }).last();
  await added.getByRole("textbox").first().fill("CUSTOM_VALUE");
  await added.getByRole("textbox").last().fill("token=a=b");
  await d.getByRole("button", { name: "添加环境变量", exact: true }).click();
  const removed = d.getByRole("group", { name: /^环境变量 \d+$/ }).last();
  await removed.getByRole("textbox").first().fill("REMOVED_VALUE");
  await removed.getByRole("textbox").last().fill("not-saved");
  await removed.getByRole("button").click();
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ }),
  ).toHaveCount(0);
  expect(instanceBodies[0].environment).toBe(
    "ALIYUN_SMS_SIGN_NAME=客户A\nCUSTOM_VALUE=token=a=b",
  );

  await page
    .getByRole("button", { name: "更多操作 lowcode-1.13a-c1", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "编辑 lowcode-1.13a-c1", exact: true })
    .click();
  const editor = page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ });
  const custom = editor.getByRole("textbox", {
    name: "CUSTOM_VALUE 的值",
    exact: true,
  });
  await expect(custom).toHaveValue("");
  await expect(custom).toHaveAttribute("placeholder", "已保存，留空保留原值");
  await expect(
    editor.getByRole("textbox", { name: "APP_PORT 的值", exact: true }),
  ).toHaveCount(0);
  await editor.getByRole("button", { name: "保存配置", exact: true }).click();
  await expect(
    page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ }),
  ).toHaveCount(0);
  expect(instanceBodies[1]).not.toHaveProperty("environment");
});

test("环境变量重复和平台托管变量在提交前校验", async ({ page }) => {
  const { writes } = await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  await d.getByRole("button", { name: "添加环境变量", exact: true }).click();
  const row = d.getByRole("group", { name: /^环境变量 \d+$/ }).last();
  await row.getByRole("textbox").first().fill("ALIYUN_SMS_SIGN_NAME");
  await row.getByRole("textbox").last().fill("duplicate");
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByText("环境变量名称重复：ALIYUN_SMS_SIGN_NAME", { exact: true }),
  ).toBeVisible();
  expect(writes).toEqual([]);
  await row.getByRole("textbox").first().fill("APP_PORT");
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByText("APP_PORT 由平台管理，不能手动覆盖。", { exact: true }),
  ).toBeVisible();
  expect(writes).toEqual([]);
  await page.screenshot({
    path: "test-results/platform-environment-desktop.png",
    fullPage: true,
    animations: "disabled",
  });
});

test("部署失败展示具体导入阶段和归档错误", async ({ page }) => {
  await setup(page, { failedLoad: true });
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  await d.getByRole("button", { name: "创建并部署" }).click();
  await page
    .getByRole("button", { name: "更多操作 lowcode-1.13a-c1", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "部署记录 lowcode-1.13a-c1", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "Docker 部署记录",
    exact: true,
  });
  await expect(dialog).toContainText("loading · failed");
  await expect(dialog.getByRole("alert")).toContainText("invalid tar header");
});

test("部署设置保存仓库连接及DockerHub加速器并预留代理", async ({ page }) => {
  const { writes } = await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  await page.getByRole("button", { name: "部署设置", exact: true }).click();
  let dialog = page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ });
  await expect(
    dialog.getByRole("tab", { name: "镜像仓库", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(dialog.getByRole("table")).toContainText("DockerHub");
  await page.screenshot({
    path: "test-results/deployment-settings-desktop.png",
    fullPage: true,
    animations: "disabled",
  });
  await dialog.getByRole("tab", { name: "代理", exact: true }).click();
  await expect(dialog).toContainText("功能预留");
  await expect(dialog.getByRole("textbox")).toHaveCount(0);
  await dialog.getByRole("tab", { name: "镜像仓库", exact: true }).click();
  await dialog.getByRole("button", { name: "加速器配置", exact: true }).click();
  await dialog.getByRole("button", { name: "＋ 添加", exact: true }).click();
  await dialog.getByLabel("加速器地址 1").fill("https://mirror.example.com");
  await dialog.getByRole("button", { name: "确定", exact: true }).click();
  await expect(dialog.getByRole("status")).toContainText("设置已保存");
  await dialog.getByRole("button", { name: "＋ 新建", exact: true }).click();
  await dialog.getByLabel("仓库别名").fill("企业仓库");
  await dialog.getByLabel("仓库地址").fill("https://registry.example.com");
  await dialog.getByLabel("仓库用户名").fill("deployer");
  await dialog.getByLabel("仓库密码").fill("secret-password");
  await dialog.getByRole("button", { name: "保存", exact: true }).click();
  await expect(dialog.getByRole("table")).toContainText("企业仓库");
  await dialog.getByRole("button", { name: "确定", exact: true }).click();
  await page.reload();
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  await page.getByRole("button", { name: "部署设置", exact: true }).click();
  dialog = page.getByRole("dialog", { name: /^(?!.*客户业务).*$/ });
  const row = dialog.getByRole("row").filter({ hasText: "企业仓库" });
  await row.getByRole("button", { name: "编辑", exact: true }).click();
  await expect(dialog.getByLabel("仓库密码")).toHaveValue("");
  await expect(dialog.getByLabel("仓库密码")).toHaveAttribute(
    "placeholder",
    "已保存，留空保留原密码",
  );
  await dialog.getByRole("button", { name: "取消", exact: true }).click();
  await dialog.getByRole("button", { name: "加速器配置", exact: true }).click();
  await expect(dialog.getByLabel("加速器地址 1")).toHaveValue(
    "https://mirror.example.com",
  );
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/deployment-settings-mobile.png",
    fullPage: true,
    animations: "disabled",
  });
  expect(writes).toContain("PUT /api/platform-registries/dockerhub");
  expect(writes).toContain("POST /api/platform-registries");
});
test("保存失败保留表单配置", async ({ page }) => {
  await setup(page, { saveError: true });
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByText("当前 Docker 环境的端口已被其他平台实例占用", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    d.getByRole("textbox", { name: "ALIYUN_SMS_SIGN_NAME 的值", exact: true }),
  ).toHaveValue("客户A");
});
test("部署提交失败重试不重复创建实例", async ({ page }) => {
  const fixture = await setup(page, { deployError: true });
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect(
    page.getByText("Docker 已停止，请启动 Docker", { exact: true }),
  ).toBeVisible();
  await d.getByRole("button", { name: "创建并部署" }).click();
  await expect.poll(() => fixture.writes.length).toBe(4);
  expect(fixture.creates()).toBe(1);
});
test("普通运营用户不能进入本机部署", async ({ page }) => {
  await setup(page, { denied: true });
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "测试客户的更多操作", exact: true })
    .click();
  await expect(
    page.getByRole("menuitem", { name: "查看平台实例", exact: true }),
  ).toHaveCount(0);
});
test("手机表单展示本机配置且不溢出", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  await fill(page);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/platform-local-docker-mobile.png",
    fullPage: true,
    animations: "disabled",
  });
});

test("实例表单按 Card 分组、同行布局并选择真实网络配置", async ({ page }) => {
  const state = await setup(page);
  await page.goto("/customers");
  await page
    .getByRole("button", { name: "查看详情 测试客户", exact: true })
    .click();
  await page
    .getByRole("dialog", { name: "测试客户 · 客户业务", exact: true })
    .getByRole("tab", { name: "平台实例", exact: true })
    .click();
  const d = await fill(page);
  for (const heading of ["基础信息", "存储空间", "网络配置", "其他"]) {
    const h = d.getByRole("heading", { name: heading, exact: true });
    await expect(h).toBeVisible();
    await expect(h.locator("..")).toHaveAttribute("data-slot", "card");
  }
  const version = await d
    .getByRole("button", { name: /平台版本包/ })
    .boundingBox();
  const customer = await d
    .getByRole("button", { name: /所属客户/ })
    .boundingBox();
  expect(Math.abs(version!.y - customer!.y)).toBeLessThan(2);
  expect(customer!.x).toBeGreaterThan(version!.x);
  const name = await d.getByLabel("实例名称", { exact: true }).boundingBox();
  const container = await d
    .getByLabel("容器名称", { exact: true })
    .boundingBox();
  expect(Math.abs(name!.y - container!.y)).toBeLessThan(2);
  await expect(
    d.getByRole("tab", { name: "bridge", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(
    d.getByRole("tab", { name: "host", exact: true }),
  ).toBeDisabled();
  await expect(
    d.getByRole("textbox", { name: "子网", exact: true }),
  ).toHaveValue("172.17.0.0/16");
  await d.getByRole("tab", { name: "macvlan", exact: true }).click();
  await expect(d.getByRole("button", { name: /网络名称/ })).toContainText(
    "customer-lan",
  );
  await expect(
    d.getByRole("textbox", { name: "子网", exact: true }),
  ).toHaveValue("192.168.50.0/24");
  await d.getByRole("button", { name: "创建并部署", exact: true }).click();
  await expect(d).toHaveCount(0);
  expect(state.instanceBodies[0]).toMatchObject({
    networkMode: "macvlan",
    networkName: "customer-lan",
  });
});

test("平台公共配置跟随版本包清单，切换旧包恢复默认挂载与端口", async ({
  page,
}, testInfo) => {
  await setup(page);
  await page.route("**:8779/api/platform-instances/options", (route) =>
    route.fulfill({
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      json: {
        data: {
          customers: [{ id: "c1", name: "客户 A" }],
          packages: [
            {
              id: pkg,
              name: "1.13a",
              environment: ["ALIYUN_SMS_SIGN_NAME="],
              deployment: {
                webPort: 8080,
                apiPort: 8081,
                environment: [],
                volumes: [
                  { key: "database", destination: "/custom/postgres" },
                  { key: "files", destination: "/custom/files" },
                ],
              },
            },
            { id: "legacy", name: "0.9" },
          ],
        },
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
  const dialog = await fill(page);
  await expect(dialog.getByLabel("挂载路径 1", { exact: true })).toHaveValue(
    "/custom/postgres",
  );
  await expect(dialog.getByLabel("挂载路径 2", { exact: true })).toHaveValue(
    "/custom/files",
  );
  await expect(dialog.getByLabel("数据卷名称 2", { exact: true })).toHaveValue(
    "yaya-实例标识-files",
  );
  await expect(dialog.getByLabel("挂载路径 1", { exact: true })).toBeDisabled();
  await expect(dialog.getByLabel("容器端口 1", { exact: true })).toHaveValue(
    "8080",
  );
  await expect(dialog.getByLabel("容器端口 2", { exact: true })).toHaveValue(
    "8081",
  );
  await expect(dialog.getByLabel("容器端口 1", { exact: true })).toBeDisabled();
  await expect(dialog.getByLabel("Web 端口", { exact: true })).toBeEnabled();
  await page.screenshot({
    path: testInfo.outputPath("platform-shared-configuration.png"),
    fullPage: true,
  });
  await dialog.getByRole("button", { name: /平台版本包/ }).click();
  await page.getByRole("option", { name: "0.9 · legacy", exact: true }).click();
  await expect(dialog.getByLabel("挂载路径 1", { exact: true })).toHaveValue(
    "/var/lib/postgresql",
  );
  await expect(dialog.getByLabel("容器端口 1", { exact: true })).toHaveValue(
    "8787",
  );
});
