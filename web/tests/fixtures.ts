import { type Page } from "@playwright/test";
import type {
  AiEmployeeProduct,
  OperationSkill,
  Order,
  Customer,
  License,
} from "../app/lib/operation-types";

export const skill: OperationSkill = {
  id: "bom-quote",
  title: "bom-quote",
  packageName: "bom-quote",
  source: "local",
  version: "1.0.0",
  packagePath: "",
  description: "报价技能",
  instructions: "读取 BOM",
  status: "active",
  createdAt: 1,
  updatedAt: 1,
};
export const employee = (
  id: string,
  status: "active" | "inactive" = "active",
  skillIds = [skill.id],
): AiEmployeeProduct => ({
  id,
  title: `员工 ${id}`,
  description: "测试员工",
  category: "engineering",
  priceCents: 300000,
  billingCycle: "year",
  version: "1.0.0",
  skillIds,
  systemPrompt: "",
  allowNetwork: false,
  allowedTools: [],
  applicationIds: [],
  status,
  createdAt: 1,
  updatedAt: 1,
});
export const customer: Customer = {
  customerId: "c1",
  name: "测试客户",
  contactName: "张三",
  contactPhone: "",
  contactEmail: "",
  status: "active",
  notes: "",
  createdAt: 1,
};
export const order: Order = {
  orderId: "o1",
  orderNo: "TEST-001",
  customerId: "c1",
  customerName: customer.name,
  items: [
    {
      kind: "platform",
      referenceId: "platform",
      title: "低代码平台",
      quantity: 1,
      unitPriceCents: 10000,
      entitlementDays: 365,
    },
  ],
  totalAmountCents: 10000,
  receivedAmountCents: 0,
  status: "pending_payment",
  createdAt: 1,
  deploymentType: "saas",
  notes: "",
};
export const license: License = {
  license: "test",
  licenseId: "l1",
  orderId: "o1",
  subject: "测试主体",
  modules: ["platform"],
  issuedAt: 1,
  expiresAt: 1900000000,
  moduleExpiresAt: {},
  moduleTitles: {},
  aiEmployees: [],
  aiEmployeeStatuses: {},
  platformStatus: "running",
  moduleStatuses: {},
};

export async function mockApi(page: Page) {
  const state = {
    employees: [employee("a"), employee("b"), employee("c", "inactive", [])],
    skills: [skill],
    orders: [structuredClone(order)],
    calls: [] as { path: string; method: string; body: unknown }[],
    fail: "",
    delay: 0,
  };
  await page.route(
    /http:\/\/(?:127\.0\.0\.1|localhost):8779\/api\//,
    async (route) => {
      const request = route.request();
      const path = new URL(request.url()).pathname;
      const headers = {
        "access-control-allow-origin": new URL(page.url()).origin,
        "access-control-allow-credentials": "true",
        "access-control-allow-headers": "content-type",
        "access-control-allow-methods": "GET,POST,PUT,DELETE,OPTIONS",
      };
      if (request.method() === "OPTIONS")
        return route.fulfill({ status: 204, headers });
      let body: unknown = null;
      try {
        body = request.postDataJSON();
      } catch {
        body = request.postData();
      }
      state.calls.push({ path, method: request.method(), body });
      if (state.delay && request.method() !== "GET")
        await new Promise((resolve) => setTimeout(resolve, state.delay));
      if (state.fail === path)
        return route.fulfill({
          status: 500,
          headers,
          contentType: "application/json",
          body: JSON.stringify({ data: null, message: "模拟请求失败" }),
        });
      let data: unknown = [];
      if (path === "/api/session")
        data = {
          user: {
            userId: "test",
            username: "test",
            displayName: "Test",
            role: "admin",
            organizationId: "test",
            status: "active",
          },
        };
      else if (path.endsWith("/deletion-eligibility"))
        data = { canDelete: true, blockers: [] };
      else if (/\/ai-employees\/[^/]+\/status$/.test(path)) {
        const target = state.employees.find((item) =>
          path.includes(`/${item.id}/`),
        );
        if (target)
          target.status = (body as { status: "active" | "inactive" }).status;
        data = target;
      } else if (path === "/api/ai-employees" && request.method() === "POST") {
        const fields = body as Partial<AiEmployeeProduct>;
        data = {
          ...employee(fields.id || "created-employee"),
          ...fields,
          id: fields.id || "created-employee",
        };
      } else if (path === "/api/ai-employees") data = state.employees;
      else if (path === "/api/platform-tools")
        data = [
          {
            id: "yaya_apply_design_patch",
            title: "修改表单设计",
            description: "写入须用户确认",
            category: "form",
            group: "搭建",
            riskLevel: "write",
          },
        ];
      else if (path === "/api/skills") data = state.skills;
      else if (path === "/api/customers") data = [customer];
      else if (path === "/api/orders") data = state.orders;
      else if (path === "/api/licenses") data = [license];
      else if (path === "/api/finance/summary")
        data = {
          totalReceivedCents: 0,
          monthReceivedCents: 0,
          outstandingCents: 10000,
          paidOrderCount: 0,
          pendingOrderCount: 1,
          activeCustomerCount: 1,
        };
      else if (path === "/api/application-products")
        data = [
          {
            productId: "p1",
            publisherSubject: "test",
            publisherAppId: "app",
            name: "测试应用",
            description: "",
            listingStatus: "unknown-state",
            entitlementCount: 0,
          },
        ];
      else if (path === "/api/skills/import" || path.endsWith("/archive"))
        data = skill;
      await route.fulfill({
        status: 200,
        headers,
        contentType: "application/json",
        body: JSON.stringify({ message: "", data }),
      });
    },
  );
  return state;
}
