export type Envelope<T> = { message: string; data: T | null };

export type OperationUser = {
  userId: string;
  username: string;
  displayName: string;
  role: string;
  organizationId: string;
  status: string;
};

export type OperationSkill = {
  id: string;
  title: string;
  packageName: string;
  source: string;
  version: string;
  packagePath: string;
  description: string;
  instructions: string;
  status: "active" | "inactive";
  createdAt: number;
  updatedAt: number;
};

export type SkillDeletionEligibility = {
  canDelete: boolean;
  blockers: Array<{ employeeId: string; employeeTitle: string; employeeStatus: string }>;
};

export type AiEmployeeProduct = {
  id: string; title: string; description: string; category: string; priceCents: number;
  billingCycle: "month" | "year" | "one_time"; version: string; packageVersion: string;
  skillIds: string[]; systemPrompt: string; allowNetwork: boolean; allowedTools: string[];
  applicationIds: string[]; avatarUrl?: string | null; status: "active" | "inactive";
  createdAt: number; updatedAt: number;
};
export type PlatformTool = { id: string; title: string; description: string; category: string; group: string; riskLevel: "read" | "write" | "external" | "destructive" };
export type AiEmployeeDeletionEligibility = { canDelete: boolean; blockers: Array<{ licenseId: string; customerName: string; expiresAt: number }> };
export type Customer = { customerId: string; name: string; contactName: string; contactPhone: string; contactEmail: string; status: "active" | "inactive"; notes: string; createdAt: number };
export type OrderItem = { kind: "platform" | "module" | "ai_employee" | "service"; referenceId?: string; title: string; quantity: number; unitPriceCents: number; entitlementDays?: number };
export type Order = { orderId: string; orderNo: string; customerId: string; customerName: string; items: OrderItem[]; totalAmountCents: number; receivedAmountCents: number; status: "pending_payment" | "paid" | "fulfilled" | "cancelled"; createdAt: number; dueAt?: number; deploymentType: "saas" | "local"; paidAt?: number; licenseId?: string; notes: string; providerId?: string };
export type Transaction = { transactionId: string; orderId: string; orderNo: string; customerId: string; customerName: string; amountCents: number; method: string; reference: string; notes: string; occurredAt: number };
export type FinanceSummary = { totalReceivedCents: number; monthReceivedCents: number; outstandingCents: number; paidOrderCount: number; pendingOrderCount: number; activeCustomerCount: number };
export type LicenseStatus = "unactivated" | "running" | "expired" | "destroyed";
export type License = { license: string; licenseId: string; orderId?: string; subject: string; customerNameSnapshot?: string; linkageStatus?: "linked" | "legacy_unlinked"; modules: string[]; issuedAt: number; expiresAt: number; moduleExpiresAt: Record<string, number>; moduleTitles: Record<string, string>; aiEmployees: Array<{ id: string; title: string; expiresAt: number }>; aiEmployeeStatuses: Record<string, LicenseStatus>; platformStatus: LicenseStatus; moduleStatuses: Record<string, LicenseStatus> };
export type ApplicationReleaseSubmission = { submissionId: string; appId: string; appName: string; version: string; status: string; submittedBy: string; applicantSubject: string; submittedAt: number; snapshot: Record<string, unknown> };
export type Provider = { providerId: string; name: string; contactName: string; contactPhone: string; status: string; createdAt: number };
export type CommissionRule = { ruleId: string; productType: string; rateBasisPoints: number; status: string; effectiveAt: number };
export type CommissionEntry = { entryId: string; orderId: string; providerId: string; productType: string; receivedAmountCents: number; rateBasisPoints: number; commissionAmountCents: number; status: string; createdAt: number };
export type AuditLog = { id: number; createdAt: number; level: "debug" | "info" | "warning" | "error"; source: string; endpointTitle: string; message: string; ipAddress?: string; method?: string; path?: string; statusCode?: number; durationMs?: number; userAgent?: string; responseBody?: string; customerId?: string; customerName?: string };
