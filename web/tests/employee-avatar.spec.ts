import { test, expect } from "@playwright/test";
import { mockApi } from "./fixtures";

const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=",
  "base64",
);

for (const editing of [false, true]) {
  test(`${editing ? "编辑" : "新增"}员工头像可预览，保存失败保留头像并按同一 ID 重试`, async ({
    page,
  }) => {
    const state = await mockApi(page);
    if (editing) state.employees[0].avatarUrl = "saved-avatar.webp";
    await page.route("**/operation-api/api/ai-employees/*/avatar?*", (route) =>
      route.fulfill({ contentType: "image/png", body: png }),
    );
    let failAvatar = true;
    const uploads: string[] = [];
    await page.route("**:8779/api/ai-employees/*/avatar", async (route) => {
      uploads.push(new URL(route.request().url()).pathname);
      const data = route.request().postDataBuffer();
      expect(data?.subarray(0, 4).toString()).toBe("RIFF");
      await route.fulfill({
        status: failAvatar ? 500 : 200,
        headers: {
          "access-control-allow-origin": "http://127.0.0.1:8780",
          "access-control-allow-credentials": "true",
        },
        contentType: "application/json",
        body: JSON.stringify({
          data: failAvatar ? null : state.employees[0],
          message: failAvatar ? "模拟头像失败" : "",
        }),
      });
    });
    await page.goto("/ai-employees");
    if (editing)
      await page
        .getByRole("row")
        .filter({ hasText: "员工 a" })
        .locator('button[aria-label="编辑"]')
        .click();
    else
      await page
        .getByRole("button", { name: "新增 AI 员工", exact: true })
        .click();
    let dialog = page.getByRole("dialog", {
      name: editing ? "编辑 AI 员工" : "新增 AI 员工",
      exact: true,
    });
    if (editing)
      await expect(
        dialog.getByRole("img", { name: "AI 员工头像", exact: true }),
      ).toHaveAttribute("src", /saved-avatar.webp/);
    const trigger = dialog.locator('button[aria-label="点击更换头像"]');
    await expect(trigger).toHaveCSS("width", "150px");
    await expect(trigger).toHaveCSS("height", "150px");
    await trigger.hover();
    await expect(page.getByRole("tooltip")).toContainText("点击更换头像");
    await trigger.click();
    const crop = page.getByRole("dialog", { name: "设置头像", exact: true });
    if (editing)
      await expect(crop.getByRole("img", { name: "当前头像" })).toBeVisible();
    await crop.locator('input[type="file"]').setInputFiles({
      name: "avatar.png",
      mimeType: "image/png",
      buffer: png,
    });
    const useAvatar = crop.getByRole("button", {
      name: "使用此头像",
      exact: true,
    });
    await expect(useAvatar).toBeEnabled();
    await useAvatar.click();
    await expect(crop).toHaveCount(0);
    await expect(
      dialog.getByRole("img", { name: "AI 员工头像", exact: true }),
    ).toHaveAttribute("src", /^blob:/);
    await expect(
      dialog.getByText("已选择新头像，保存 AI 员工后生效。"),
    ).toHaveCount(0);
    expect(uploads).toHaveLength(0);
    if (!editing) {
      await dialog.getByLabel("展示名称", { exact: true }).fill("头像员工");
      await dialog.getByLabel("销售价格", { exact: true }).fill("100");
    }
    await dialog
      .getByRole("button", { name: "保存 AI 员工", exact: true })
      .click();
    dialog = page.getByRole("dialog", { name: "编辑 AI 员工", exact: true });
    await expect(
      dialog.getByText(/AI 员工信息已保存，但头像上传失败/),
    ).toBeVisible();
    await expect(
      dialog.getByRole("img", { name: "AI 员工头像", exact: true }),
    ).toHaveAttribute("src", /^blob:/);
    failAvatar = false;
    await dialog
      .getByRole("button", { name: "保存 AI 员工", exact: true })
      .click();
    await expect(dialog).toHaveCount(0);
    expect(uploads).toEqual([
      `/api/ai-employees/${editing ? "a" : "created-employee"}/avatar`,
      `/api/ai-employees/${editing ? "a" : "created-employee"}/avatar`,
    ]);
    expect(
      state.calls
        .filter(
          (call) => call.path === "/api/ai-employees" && call.method === "POST",
        )
        .at(-1)?.body,
    ).toMatchObject({ id: editing ? "a" : "created-employee" });
  });
}
