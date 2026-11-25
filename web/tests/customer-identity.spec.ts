import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

test("客户 ID 可复制，发布客户匹配名称并保留历史主体 ID", async ({
  page,
  context,
}) => {
  await mockApi(page);
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.goto("/customers");
  const customer = page.getByRole("row").filter({ hasText: "测试客户" });
  await expect(customer).toContainText("客户 ID：c1");
  await customer
    .getByRole("button", { name: "复制客户 ID：c1", exact: true })
    .click();
  await expect
    .poll(() => page.evaluate(() => navigator.clipboard.readText()))
    .toBe("c1");
  await page.route("**/api/application-products", (route) =>
    route.fulfill({
      json: {
        message: "",
        data: [
          {
            productId: "p1",
            name: "关联应用",
            publisherSubject: "c1",
            version: "1.0.0",
            listingStatus: "listed",
            entitlementCount: 0,
          },
          {
            productId: "p2",
            name: "历史应用",
            publisherSubject: "legacy-customer",
            version: "1.0.0",
            listingStatus: "listed",
            entitlementCount: 0,
          },
        ],
      },
    }),
  );
  await page.goto("/application-products");
  await expect(
    page.getByRole("columnheader", { name: "发布客户", exact: true }),
  ).toBeVisible();
  const linked = page.getByRole("row").filter({ hasText: "关联应用" });
  await expect(linked).toContainText("测试客户");
  await expect(linked).toContainText("客户 ID：c1");
  const legacy = page.getByRole("row").filter({ hasText: "历史应用" });
  await expect(legacy).toContainText("未关联客户");
  await expect(legacy).toContainText("客户 ID：legacy-customer");
});
