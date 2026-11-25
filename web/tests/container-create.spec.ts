import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

for (const width of [1440, 390]) {
  test(`通用容器公共配置编辑及提交 ${width}px`, async ({ page }, testInfo) => {
    await mockApi(page);
    // HTTP IP deployments lack randomUUID while getRandomValues remains usable.
    await page.addInitScript(() => {
      Object.defineProperty(window.crypto, "randomUUID", {
        value: undefined,
        configurable: true,
      });
    });
    const pageErrors: string[] = [];
    page.on("pageerror", (error) => pageErrors.push(error.message));
    await page.setViewportSize({ width, height: 960 });
    const headers = {
      "access-control-allow-origin": "http://127.0.0.1:8780",
      "access-control-allow-credentials": "true",
    };
    await page.route("**:8779/api/session", (route) =>
      route.fulfill({
        headers,
        json: {
          data: {
            user: {
              userId: "admin",
              role: "platform_admin",
              username: "admin",
              displayName: "管理员",
            },
          },
        },
      }),
    );
    const bodies: Record<string, unknown>[] = [];
    await page.route("**:8779/api/ops/local/**", (route) => {
      const path = new URL(route.request().url()).pathname;
      let data: unknown = [];
      if (path.endsWith("/inventory"))
        data = {
          info: { operatingSystem: "Linux", serverVersion: "27" },
          images: [
            {
              id: `sha256:${"a".repeat(64)}`,
              repository: "nginx",
              tag: "latest",
            },
          ],
          networks: [],
          volumes: [],
          disk: [],
        };
      if (path.endsWith("/containers")) {
        data = { items: [], total: 0, limit: 100, truncated: false };
        if (route.request().method() === "POST") {
          bodies.push(route.request().postDataJSON());
          data = {
            creationId: bodies[0].creationId,
            status: "failed",
            error: "模拟 Docker 创建失败",
          };
        }
      }
      return route.fulfill({ headers, json: { data } });
    });
    await page.route("**:8779/api/platform-networks", (route) =>
      route.fulfill({
        headers,
        json: {
          data: {
            items: [
              { name: "bridge", driver: "bridge", subnets: ["172.17.0.0/16"] },
            ],
            hostSupported: true,
          },
        },
      }),
    );
    await page.goto("/ops/containers/containers");
    await page.getByRole("button", { name: "新增容器", exact: true }).click();
    expect(pageErrors).toEqual([]);
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("button", { name: /镜像/ }).click();
    await page
      .getByRole("option", { name: "nginx:latest", exact: true })
      .click();
    await dialog.getByLabel("容器名称", { exact: true }).fill("nginx-demo");
    await dialog
      .getByRole("button", { name: "添加数据卷", exact: true })
      .click();
    await dialog.getByLabel("数据卷名称 1", { exact: true }).fill("nginx-data");
    await dialog.getByLabel("挂载路径 1", { exact: true }).fill("/data");
    await dialog
      .getByRole("button", { name: "添加端口映射", exact: true })
      .click();
    await dialog.getByLabel("宿主端口 1", { exact: true }).fill("9080");
    await dialog.getByLabel("容器端口 1", { exact: true }).fill("80");
    await dialog
      .getByRole("button", { name: "添加环境变量", exact: true })
      .click();
    await dialog
      .getByLabel("环境变量名 1", { exact: true })
      .fill("custom_value");
    await dialog
      .getByLabel("custom_value 的值", { exact: true })
      .fill("token=a=b");
    await dialog
      .getByRole("button", { name: "添加环境变量", exact: true })
      .click();
    await dialog
      .getByLabel("环境变量名 2", { exact: true })
      .fill("custom_value");
    await dialog
      .getByRole("button", { name: "创建并启动", exact: true })
      .click();
    await expect(
      page.getByText("环境变量名称重复：custom_value", { exact: true }),
    ).toBeVisible();
    expect(bodies).toHaveLength(0);
    await dialog
      .getByRole("button", { name: "移除环境变量 2", exact: true })
      .click();
    await dialog.getByRole("tab", { name: "host", exact: true }).click();
    await expect(dialog.getByLabel("宿主端口 1", { exact: true })).toHaveCount(
      0,
    );
    await dialog.getByRole("tab", { name: "bridge", exact: true }).click();
    await expect(dialog.getByLabel("宿主端口 1", { exact: true })).toHaveValue(
      "9080",
    );
    expect(
      await dialog.evaluate(
        (element) => element.scrollWidth <= element.clientWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: testInfo.outputPath("container-create.png"),
      fullPage: true,
    });
    await dialog
      .getByRole("button", { name: "创建并启动", exact: true })
      .click();
    await expect.poll(() => bodies.length).toBe(1);
    expect(bodies[0]).toMatchObject({
      name: "nginx-demo",
      networkMode: "bridge",
      networkName: "bridge",
      ports: [{ hostPort: 9080, containerPort: 80, protocol: "tcp" }],
      volumes: [{ name: "nginx-data", destination: "/data", readOnly: false }],
      environment: ["custom_value=token=a=b"],
      restartPolicy: "unless-stopped",
      start: true,
    });
    await expect(
      dialog.getByLabel("环境变量名 1", { exact: true }),
    ).toBeDisabled();
    await expect(
      dialog.getByLabel("数据卷名称 1", { exact: true }),
    ).toBeDisabled();
  });
}
