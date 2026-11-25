import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

test("官方发布方可导入及选择官方商品升级，并在列表和申请中显示名称", async ({
  page,
}) => {
  await mockApi(page);
  await page.route("**/api/customers", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.route("**/api/application-products", (route) =>
    route.fulfill({
      json: {
        data: [
          {
            productId: "official1",
            name: "官方应用",
            publisherSubject: "yaya_official",
            listingStatus: "withdrawn",
            members: [],
            dependencies: [],
          },
        ],
      },
    }),
  );
  await page.route("**/api/application-submissions", (route) =>
    route.fulfill({
      json: {
        data: [
          {
            submissionId: "s1",
            appName: "官方应用",
            version: "1.0.0",
            applicantSubject: "yaya_official",
            submittedBy: "运营导入：admin",
            status: "pending_review",
            snapshot: {},
            submittedAt: 1,
          },
        ],
      },
    }),
  );
  let imported = false;
  await page.route("**/api/application-products/import", async (route) => {
    const body = route.request().postDataBuffer()?.toString() ?? "";
    expect(body).toContain("yaya_official");
    expect(body).toContain("official1");
    imported = true;
    await route.fulfill({ json: { data: null } });
  });
  await page.goto("/application-products");
  await expect(
    page.getByText("丫丫运营平台（官方）", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "导入应用包", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "导入应用包", exact: true });
  await dialog.getByRole("button", { name: /发布客户/ }).click();
  await page
    .getByRole("option", { name: "丫丫运营平台（官方）", exact: true })
    .click();
  await dialog.getByRole("button", { name: /目标商品/ }).click();
  await page
    .getByRole("option", { name: "官方应用 · 新版本", exact: true })
    .click();
  await dialog.getByRole("textbox", { name: "商品说明" }).fill("官方版本升级");
  await dialog
    .getByLabel("应用包 ZIP")
    .setInputFiles({
      name: "official.zip",
      mimeType: "application/zip",
      buffer: Buffer.from("fixture"),
    });
  await dialog
    .getByRole("button", { name: "导入并进入审核", exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  expect(imported).toBe(true);
  await page
    .getByRole("button", { name: "应用上线申请管理", exact: true })
    .click();
  await expect(
    page.getByRole("dialog").getByText("丫丫运营平台（官方）", { exact: true }),
  ).toBeVisible();
});
