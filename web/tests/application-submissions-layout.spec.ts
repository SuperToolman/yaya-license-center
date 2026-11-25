import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

test("申请类型 Chip 直接展开范围，宽表格横向滚动", async ({
  page,
}, testInfo) => {
  await mockApi(page);
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.route("**/api/application-submissions", (route) =>
    route.fulfill({
      json: {
        data: ["single", "suite"].map((type) => ({
          submissionId: type,
          appId: `app-${type}`,
          appName: type === "single" ? "字典中心" : "销售套件",
          version: "1.0.0",
          status: "pending_review",
          submittedBy: "运营导入：usr-aedaa477-ee71-4c77-bcb5-e1e042e70c93",
          applicantSubject: "c1",
          submittedAt: 1794888000,
          snapshot: {
            commercial: {
              pricingMode: "paid",
              priceCents: 19900,
              publisherRateBasisPoints: 7500,
            },
            manifest: {
              applicationType: type,
              members: [
                {
                  appId: `app-${type}`,
                  resources: { apps: [{ name: "成员应用" }] },
                },
              ],
              dependencies: [],
            },
          },
        })),
      },
    }),
  );
  await page.goto("/application-products");
  await page
    .getByRole("button", { name: "应用上线申请管理", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "应用上线申请管理",
    exact: true,
  });
  await expect(dialog.getByRole("columnheader")).toHaveCount(9);
  await expect(
    dialog.getByRole("columnheader", { name: "申请客户&提交人", exact: true }),
  ).toBeVisible();
  const applicantCell = dialog
    .getByRole("row")
    .filter({ hasText: "字典中心" })
    .getByRole("gridcell")
    .filter({ hasText: "测试客户" });
  await expect(applicantCell).toContainText("测试客户 · 运营导入");
  await expect(applicantCell).not.toContainText("usr-aedaa477");
  await expect(applicantCell).toContainText("c1");
  await expect(dialog.getByText("查看应用与依赖", { exact: true })).toHaveCount(
    0,
  );
  for (const type of ["单应用", "套应用"]) {
    const trigger = dialog.getByRole("button", {
      name: `${type}：展开成员与依赖`,
    });
    const chip = trigger.locator('[data-slot="chip"]');
    await expect(chip).toHaveText(type);
    expect(
      await chip.evaluate(
        (element) => getComputedStyle(element).backgroundColor,
      ),
    ).not.toBe("rgba(0, 0, 0, 0)");
    await expect
      .poll(() =>
        trigger.evaluate((element) => {
          const chip = element
            .querySelector('[data-slot="chip"]')!
            .getBoundingClientRect();
          const arrow = element.querySelector("svg")!.getBoundingClientRect();
          return Math.abs(
            chip.y + chip.height / 2 - arrow.y - arrow.height / 2,
          );
        }),
      )
      .toBeLessThan(2);
    await trigger.click();
    await expect(trigger).toHaveAttribute("aria-expanded", "true");
    await expect(
      trigger.locator("xpath=ancestor::td").getByText(/成员应用（app-/),
    ).toBeVisible();
    await trigger.click();
    await expect(trigger).toHaveAttribute("aria-expanded", "false");
  }
  await dialog.locator(".table__scroll-container").evaluate((element) => {
    element.scrollLeft = 0;
  });
  await page.screenshot({
    path: testInfo.outputPath("submissions-desktop.png"),
  });
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 });
    const scroll = dialog.locator(".table__scroll-container");
    expect(
      await scroll.evaluate(
        (element) => element.scrollWidth > element.clientWidth,
      ),
    ).toBe(true);
    expect(
      await dialog.evaluate((element) => element.getBoundingClientRect().right),
    ).toBeLessThanOrEqual(width);
    await scroll.evaluate((element) => {
      element.scrollLeft = element.scrollWidth;
    });
    await expect(
      dialog.getByRole("button", { name: "同意", exact: true }).first(),
    ).toBeInViewport();
  }
});
