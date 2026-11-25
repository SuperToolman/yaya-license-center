import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";

test("套应用显示完整范围，上架必须单独选择审核通过版本", async ({ page }) => {
  await mockApi(page);
  const changes: unknown[] = [];
  await page.route("**/api/application-products**", async (route) => {
    const url = route.request().url();
    if (route.request().method() === "POST") {
      changes.push(route.request().postDataJSON());
      await route.fulfill({
        json: { code: 0, data: null, message: "上架完成" },
      });
      return;
    }
    if (url.endsWith("/releases")) {
      await route.fulfill({
        json: {
          data: [
            { submissionId: "release-1", version: "1.0.0", submittedAt: 1 },
          ],
        },
      });
      return;
    }
    await route.fulfill({
      json: {
        data: [
          {
            productId: "suite",
            publisherSubject: "c1",
            publisherAppId: "a",
            rootAppId: "a",
            name: "销售套件",
            description: "",
            version: "1.0.0",
            releaseId: "",
            listingStatus: "withdrawn",
            entitlementCount: 0,
            pendingCount: 1,
            applicationType: "suite",
            members: [
              { appId: "a", name: "销售" },
              { appId: "b", name: "客户" },
            ],
            dependencies: [
              {
                sourceAppId: "a",
                targetAppId: "b",
                path: "/fields/client",
                resourceId: "client",
              },
            ],
          },
        ],
      },
    });
  });
  await page.goto("/application-products");
  await expect(page.getByText("2 个成员", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { name: "销售套件：展开成员与依赖", exact: true })
    .click();
  await expect(page.getByText("客户（b）", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "上架", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "选择 销售套件 的上架版本" });
  await expect(dialog).toBeVisible();
  expect(changes).toHaveLength(0);
  await dialog.getByRole("button", { name: /v1.0.0/ }).click();
  await expect
    .poll(() => changes)
    .toEqual([{ listed: true, releaseId: "release-1" }]);
});

test("应用管理在模态框中审核申请并同步刷新商品，失败保留驳回原因", async ({
  page,
}) => {
  const state = await mockApi(page);
  const submissions = [
    { submissionId: "s1", appName: "待通过应用", status: "pending_review" },
    { submissionId: "s2", appName: "待驳回应用", status: "pending_review" },
  ].map((item) => ({
    ...item,
    appId: item.submissionId,
    version: "1.0.0",
    submittedBy: "测试提交人",
    applicantSubject: "c1",
    submittedAt: 1,
    snapshot: {
      submissionType:
        item.submissionId === "s1" ? "new_listing" : "version_upgrade",
      source: "online",
    },
  }));
  let failReview = false;
  const reviews: unknown[] = [];
  await page.route("**/api/application-submissions**", async (route) => {
    if (route.request().method() === "POST") {
      const body = route.request().postDataJSON();
      reviews.push(body);
      if (failReview) {
        await route.fulfill({
          status: 500,
          json: { message: "审核失败，请重试", data: null },
        });
        return;
      }
      const submission = submissions.find((item) =>
        route.request().url().includes(`/${item.submissionId}/review`),
      )!;
      submission.status = body.approved ? "approved" : "rejected";
      await route.fulfill({ json: { message: "", data: submission } });
      return;
    }
    await route.fulfill({ json: { message: "", data: submissions } });
  });
  await page.goto("/application-products");
  await expect(
    page.getByRole("heading", { name: "应用管理", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("navigation", { name: "主导航" })
      .getByRole("link", { name: "应用上线申请管理" }),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "应用上线申请管理", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "应用上线申请管理",
    exact: true,
  });
  await expect(
    dialog.getByText("测试客户", { exact: true }).first(),
  ).toBeVisible();
  const productsBefore = state.calls.filter(
    (call) => call.path === "/api/application-products",
  ).length;
  await dialog
    .getByRole("row")
    .filter({ hasText: "待通过应用" })
    .getByRole("button", { name: "同意", exact: true })
    .click();
  await expect(
    dialog
      .getByRole("row")
      .filter({ hasText: "待通过应用" })
      .getByRole("button", { name: "同意", exact: true }),
  ).toHaveCount(0);
  await expect
    .poll(
      () =>
        state.calls.filter((call) => call.path === "/api/application-products")
          .length,
    )
    .toBeGreaterThan(productsBefore);
  expect(reviews[0]).toEqual({ approved: true, reason: "" });
  await dialog
    .getByRole("row")
    .filter({ hasText: "待驳回应用" })
    .getByRole("button", { name: "驳回", exact: true })
    .click();
  await expect(dialog.getByRole("button", { name: "确认驳回" })).toBeDisabled();
  await dialog
    .getByRole("textbox", { name: "驳回原因" })
    .fill(" 缺少上线资料 ");
  failReview = true;
  await dialog.getByRole("button", { name: "确认驳回" }).click();
  await expect(dialog.getByRole("alert")).toContainText("审核失败，请重试");
  await expect(dialog.getByRole("textbox", { name: "驳回原因" })).toHaveValue(
    " 缺少上线资料 ",
  );
  failReview = false;
  await dialog.getByRole("button", { name: "确认驳回" }).click();
  await expect(dialog.getByRole("textbox", { name: "驳回原因" })).toHaveCount(
    0,
  );
  expect(reviews.at(-1)).toEqual({ approved: false, reason: "缺少上线资料" });
  await expect(page).toHaveURL(/\/application-products$/);
  await dialog.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page
    .getByRole("button", { name: "应用上线申请管理", exact: true })
    .click();
  await expect(
    dialog.getByRole("button", { name: "同意", exact: true }),
  ).toHaveCount(0);
});

test("离线应用包选择发布客户，导入后进入审核而不自动上架", async ({ page }) => {
  await mockApi(page);
  let imported = false;
  await page.route("**/api/application-products/import", async (route) => {
    expect(route.request().headers()["content-type"]).toContain(
      "multipart/form-data",
    );
    const body = route.request().postDataBuffer()?.toString() ?? "";
    expect(body).toContain('name="publisherSubject"');
    expect(body).toContain("c1");
    expect(body).toContain('filename="offline-suite.zip"');
    imported = true;
    await route.fulfill({
      json: {
        data: { submissionId: "offline-1", status: "pending_review" },
        message: "已进入审核",
      },
    });
  });
  await page.goto("/application-products");
  await page.getByRole("button", { name: "导入应用包", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "导入应用包" });
  await expect(
    dialog.getByText("申请类型：新上线", { exact: true }),
  ).toBeVisible();
  await expect(dialog.getByText(/支持完整单应用或套应用 V3 ZIP/)).toBeVisible();
  await expect(
    dialog.getByRole("button", { name: "导入并进入审核" }),
  ).toBeDisabled();
  await dialog.getByRole("button", { name: /发布客户/ }).click();
  await page.getByRole("option", { name: "测试客户", exact: true }).click();
  await dialog.getByLabel("商品说明").fill("离线套应用说明");
  await dialog.getByLabel("应用包 ZIP").setInputFiles({
    name: "offline-suite.zip",
    mimeType: "application/zip",
    buffer: Buffer.from("fixture"),
  });
  await dialog.getByRole("button", { name: "导入并进入审核" }).click();
  await expect.poll(() => imported).toBe(true);
  await expect(dialog).toHaveCount(0);
});

test("商业配置必须包含售价和分成，审核后通过显式上架生效", async ({ page }) => {
  await mockApi(page);
  const config = {
    pricingMode: "paid",
    priceCents: 19901,
    publisherRateBasisPoints: 7500,
    currency: "CNY",
    licenseModel: "one_time",
    description: "完整套应用",
  };
  let versions: unknown[] = [];
  const actions: unknown[] = [];
  await page.route("**/api/application-products", (r) =>
    r.fulfill({
      json: {
        data: [
          {
            productId: "suite",
            publisherSubject: "c1",
            publisherAppId: "a",
            rootAppId: "a",
            name: "销售套件",
            version: "1.0.0",
            releaseId: "release-1",
            listingStatus: "withdrawn",
            applicationType: "suite",
            members: [],
            dependencies: [],
            pendingCount: 0,
          },
        ],
      },
    }),
  );
  await page.route("**/api/application-submissions", (r) =>
    r.fulfill({
      json: {
        data: [
          {
            submissionId: "release-1",
            productId: "suite",
            version: "1.0.0",
            status: "approved",
          },
        ],
      },
    }),
  );
  await page.route(
    "**/api/application-products/suite/commercial",
    async (r) => {
      if (r.request().method() === "POST") {
        expect(r.request().postDataJSON()).toEqual({
          releaseId: "release-1",
          config,
        });
        versions = [
          {
            id: "commercial-1",
            releaseId: "release-1",
            config,
            status: "pending_review",
          },
        ];
        await r.fulfill({ json: { data: null } });
      } else await r.fulfill({ json: { data: versions } });
    },
  );
  await page.route(
    "**/api/application-commercial/commercial-1/review",
    async (r) => {
      actions.push(r.request().postDataJSON());
      versions = [
        {
          id: "commercial-1",
          releaseId: "release-1",
          config,
          status: "approved",
        },
      ];
      await r.fulfill({ json: { data: null } });
    },
  );
  await page.goto("/application-products");
  await page.getByRole("button", { name: /的更多操作/ }).click();
  await page.getByRole("menuitem", { name: "销售配置", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "销售套件 · 销售配置" });
  await dialog.getByRole("button", { name: /关联发布版本/ }).click();
  await page
    .getByRole("option", { name: "v1.0.0 · 已通过", exact: true })
    .click();
  await dialog.getByLabel("商品说明").fill("完整套应用");
  await dialog.getByRole("button", { name: /定价方式/ }).click();
  await page.getByRole("option", { name: "一次买断", exact: true }).click();
  await dialog.getByLabel("整包售价", { exact: true }).fill("199.01");
  await dialog.getByLabel("发布者分成", { exact: true }).fill("75");
  await dialog
    .getByRole("button", { name: "提交商业配置", exact: true })
    .click();
  await dialog
    .getByRole("button", { name: "通过商业审核", exact: true })
    .click();
  await expect.poll(() => actions).toEqual([{ approved: true, reason: "" }]);
  await expect(dialog.getByText(/重新选择发布版本上架/)).toBeVisible();
});

test("运营交易页可审核模拟退款并登记发布者模拟结算", async ({ page }) => {
  await mockApi(page);
  let status = "refund_requested";
  let settled = false;
  await page.route("**/api/application-commerce", (r) =>
    r.fulfill({
      json: {
        data: {
          orders: [
            {
              orderId: "order-1",
              subject: "c1",
              productId: "suite",
              status,
              amountCents: 19900,
              snapshot: { name: "销售套件" },
              refundReason: "不再需要",
            },
          ],
          revenues: [
            {
              id: "rev-1",
              publisherSubject: "publisher",
              orderId: "order-1",
              amountCents: 15920,
              status: settled ? "settled" : "pending_settlement",
            },
          ],
          settlements: settled
            ? [
                {
                  id: "batch-1",
                  publisherSubject: "publisher",
                  amountCents: 15920,
                  status: "simulated",
                },
              ]
            : [],
        },
      },
    }),
  );
  await page.route("**/api/application-commerce/order-1/refund", async (r) => {
    expect(r.request().postDataJSON()).toEqual({ approved: true, reason: "" });
    status = "refunded";
    await r.fulfill({ json: { data: null } });
  });
  await page.route("**/api/application-commerce/settlements", async (r) => {
    expect(r.request().postDataJSON()).toEqual({
      publisherSubject: "publisher",
    });
    settled = true;
    await r.fulfill({ json: { data: null } });
  });
  await page.goto("/application-products");
  await page.getByRole("button", { name: "交易与分成", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "应用交易与分成" });
  await expect(dialog.getByText("退款原因：不再需要")).toBeVisible();
  await dialog
    .getByRole("button", { name: "同意模拟退款", exact: true })
    .click();
  await expect(dialog.getByText("refunded", { exact: true })).toBeVisible();
  await dialog
    .getByRole("button", { name: "登记模拟结算", exact: true })
    .click();
  await expect(dialog.getByText(/模拟结算已登记/)).toBeVisible();
});

test("上架入口不重复，已上架商品可直接切换版本或下架", async ({ page }) => {
  await mockApi(page);
  let listed = false;
  let releaseId = "";
  const actions: unknown[] = [];
  await page.route("**/api/application-products**", async (route) => {
    if (route.request().method() === "POST") {
      const body = route.request().postDataJSON();
      actions.push(body);
      listed = body.listed;
      if (body.releaseId) releaseId = body.releaseId;
      await route.fulfill({ json: { data: null } });
    } else if (route.request().url().endsWith("/releases")) {
      await route.fulfill({
        json: {
          data: ["1", "2"].map((version) => ({
            submissionId: `r${version}`,
            version: `${version}.0.0`,
            submittedAt: 1,
          })),
        },
      });
    } else {
      await route.fulfill({
        json: {
          data: [
            {
              productId: "p1",
              name: "换版应用",
              publisherSubject: "c1",
              listingStatus: listed ? "listed" : "withdrawn",
              releaseId,
              version: releaseId === "r2" ? "2.0.0" : "1.0.0",
              members: [],
              dependencies: [],
            },
          ],
        },
      });
    }
  });
  await page.goto("/application-products");
  await expect(
    page.getByRole("button", { name: "版本", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "切换版本", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "上架", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "选择 换版应用 的上架版本" });
  await expect(dialog).toBeVisible();
  expect(actions).toHaveLength(0);
  await dialog.getByRole("button", { name: /v1.0.0/ }).click();
  await expect(
    page.getByRole("button", { name: "下架", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: /的更多操作/ }).click();
  await page.getByRole("menuitem", { name: "切换版本", exact: true }).click();
  await expect(dialog).toBeVisible();
  expect(actions).toEqual([{ listed: true, releaseId: "r1" }]);
  await dialog.getByRole("button", { name: /v2.0.0/ }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByRole("gridcell").filter({ hasText: "v2.0.0" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "下架", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "上架", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "切换版本", exact: true }),
  ).toHaveCount(0);
  expect(actions).toEqual([
    { listed: true, releaseId: "r1" },
    { listed: true, releaseId: "r2" },
    { listed: false },
  ]);
});
