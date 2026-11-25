import { test, expect } from "@playwright/test";
import { mockApi } from "./fixtures";

test("任意 Skill 按真实变量名动态渲染，配置独立保存回显和清空", async ({
  page,
}) => {
  const state = await mockApi(page);
  state.skills = structuredClone(state.skills);
  state.skills[0].environmentVariables = [
    { name: "CUSTOM_ENDPOINT", secret: false },
    { name: "ACCESS_TOKEN", secret: true },
  ];
  state.skills[0].environmentValues = {
    CUSTOM_ENDPOINT: "https://example.com",
    ACCESS_TOKEN: "test-only-token",
  };
  await page.route("**:8779/api/skills", async (route) => {
    if (route.request().method() !== "POST") return route.fallback();
    const body = route.request().postDataJSON();
    expect(body).not.toHaveProperty("componentLibraryApiUrl");
    expect(body.environmentValues).not.toHaveProperty(
      "COMPONENT_LIBRARY_API_URL",
    );
    Object.assign(state.skills[0], body);
    await route.fulfill({
      contentType: "application/json",
      headers: {
        "access-control-allow-origin": "http://127.0.0.1:8780",
        "access-control-allow-credentials": "true",
      },
      body: JSON.stringify({ message: "已保存", data: state.skills[0] }),
    });
  });
  await page.goto("/skills");
  const open = () =>
    page.locator('button[aria-label="编辑 bom-quote"]').click();
  const modal = page.getByRole("dialog", { name: "编辑 Skill", exact: true });
  const input = modal.getByLabel("CUSTOM_ENDPOINT", { exact: true });
  await open();
  await expect(
    modal.getByLabel("元器件资料库地址", { exact: true }),
  ).toHaveCount(0);
  await expect(input).toHaveValue("https://example.com");
  await expect(
    modal.getByLabel("ACCESS_TOKEN", { exact: true }),
  ).toHaveAttribute("type", "password");
  await input.fill("https://another.example.com/api");
  await modal.getByRole("button", { name: "保存 Skill", exact: true }).click();
  await expect(modal).not.toBeVisible();
  await open();
  await expect(input).toHaveValue("https://another.example.com/api");
  await input.fill("");
  await modal.getByRole("button", { name: "保存 Skill", exact: true }).click();
  await expect(modal).not.toBeVisible();
  await open();
  await expect(input).toHaveValue("");
  await expect(modal.getByLabel("ACCESS_TOKEN", { exact: true })).toHaveValue(
    "test-only-token",
  );
});

test("没有环境变量的 Skill 不显示配置项，即使存在历史业务字段", async ({
  page,
}) => {
  const state = await mockApi(page);
  state.skills = structuredClone(state.skills);
  Object.assign(state.skills[0], {
    componentLibraryApiUrl: "http://localhost:8737",
  });
  await page.goto("/skills");
  await page.locator('button[aria-label="编辑 bom-quote"]').click();
  const modal = page.getByRole("dialog", { name: "编辑 Skill", exact: true });
  await expect(modal).toBeVisible();
  await expect(modal.locator("input[name]")).toHaveCount(0);
  await expect(
    modal.getByLabel("元器件资料库地址", { exact: true }),
  ).toHaveCount(0);
});

test("元器件库配置也使用环境变量名称，不显示业务专用字段", async ({ page }) => {
  const state = await mockApi(page);
  state.skills = structuredClone(state.skills);
  state.skills[0].environmentVariables = [
    { name: "COMPONENT_LIBRARY_API_URL", secret: false },
  ];
  await page.goto("/skills");
  await page.locator('button[aria-label="编辑 bom-quote"]').click();
  const modal = page.getByRole("dialog", { name: "编辑 Skill", exact: true });
  await expect(
    modal.getByLabel("COMPONENT_LIBRARY_API_URL", { exact: true }),
  ).toBeVisible();
  await expect(
    modal.getByLabel("CUSTOM_ENDPOINT", { exact: true }),
  ).toHaveCount(0);
});

for (const viewport of [
  { width: 1280, height: 720 },
  { width: 390, height: 650 },
]) {
  test(`长表单在正文内滚动，底部操作始终可见 ${viewport.width}`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    const state = await mockApi(page);
    state.skills = structuredClone(state.skills);
    state.skills[0].environmentVariables = Array.from(
      { length: 20 },
      (_, index) => ({
        name: `CONFIG_${index}`,
        secret: false,
      }),
    );
    await page.goto("/skills");
    await page.locator('button[aria-label="编辑 bom-quote"]').click();
    const modal = page.getByRole("dialog", { name: "编辑 Skill", exact: true });
    const save = modal.getByRole("button", { name: "保存 Skill", exact: true });
    await expect(save).toBeInViewport();
    const body = modal.locator(".modal__body");
    await expect
      .poll(() =>
        body.evaluate((node) => node.scrollHeight > node.clientHeight),
      )
      .toBe(true);
    expect(
      await modal
        .getByLabel("执行说明", { exact: true })
        .evaluate((node) =>
          Boolean(
            node.compareDocumentPosition(
              document.querySelector('input[name="CONFIG_0"]')!,
            ) & Node.DOCUMENT_POSITION_FOLLOWING,
          ),
        ),
    ).toBe(true);
    await modal
      .getByLabel("CONFIG_19", { exact: true })
      .scrollIntoViewIfNeeded();
    await expect(save).toBeInViewport();
    await expect(
      modal.getByLabel("CONFIG_19", { exact: true }),
    ).toBeInViewport();
    await expect(
      save.locator("xpath=ancestor::*[contains(@class, 'modal__footer')]"),
    ).toHaveCount(1);
    const bounds = await modal.boundingBox();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(viewport.width);
    if (viewport.width === 1280) expect(bounds!.width).toBe(960);
  });
}
