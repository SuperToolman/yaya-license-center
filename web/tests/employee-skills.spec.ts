import { test, expect } from "@playwright/test";
import { mockApi, skill } from "./fixtures";
import { formatDate } from "../app/lib/format";

for (const editing of [false, true]) {
  test(`${editing ? "编辑" : "新增"}员工：同名 Skills 独立选择并保存，文本域填满整行`, async ({
    page,
  }) => {
    const state = await mockApi(page);
    const second = {
      ...skill,
      id: "bom-quote-second",
      createdAt: 1793232000,
      updatedAt: 1793232100,
    };
    state.skills.push(second);
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
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect
      .poll(async () => Math.round((await dialog.boundingBox())!.width))
      .toBe(1120);
    const group = dialog.getByRole("group", {
      name: "封装 Skills",
      exact: true,
    });
    const firstCheckbox = group.getByRole("checkbox", {
      name: "bom-quote（bom-quote）",
      exact: true,
    });
    const secondCheckbox = group.getByRole("checkbox", {
      name: "bom-quote（bom-quote-second）",
      exact: true,
    });
    const firstControl = group.locator('[data-slot="checkbox-content"]').nth(0);
    const secondControl = group
      .locator('[data-slot="checkbox-content"]')
      .nth(1);
    await expect(firstCheckbox).toBeChecked({ checked: editing });
    await expect(secondCheckbox).not.toBeChecked();
    if (!editing) await firstControl.click();
    await expect(firstCheckbox).toBeChecked();
    await expect(secondCheckbox).not.toBeChecked();
    await secondControl.click();
    await firstControl.click();
    await expect(firstCheckbox).not.toBeChecked();
    await expect(secondCheckbox).toBeChecked();
    await expect(
      group.getByText("ID：bom-quote-second", { exact: true }),
    ).toBeVisible();
    await expect(
      group.getByText(`更新时间：${formatDate(second.updatedAt)}`, {
        exact: true,
      }),
    ).toBeVisible();
    for (const [label, minimumHeight] of [
      ["AI 员工描述", 90],
      ["人格提示词", 150],
    ] as const) {
      const input = dialog.getByLabel(label, { exact: true });
      const bounds = await input.evaluate((element) => ({
        width: element.getBoundingClientRect().width,
        parentWidth: element.parentElement!.getBoundingClientRect().width,
        height: element.getBoundingClientRect().height,
      }));
      expect(Math.abs(bounds.width - bounds.parentWidth)).toBeLessThan(2);
      expect(bounds.height).toBeGreaterThan(minimumHeight);
    }
    if (!editing) {
      await dialog.getByLabel("展示名称", { exact: true }).fill("新员工");
      await dialog.getByLabel("销售价格", { exact: true }).fill("100");
    }
    await dialog
      .getByRole("button", { name: "保存 AI 员工", exact: true })
      .click();
    await expect(dialog).toHaveCount(0);
    expect(
      state.calls.findLast(
        (call) => call.path === "/api/ai-employees" && call.method === "POST",
      )?.body,
    ).toMatchObject({ skillIds: [second.id] });
  });
}
