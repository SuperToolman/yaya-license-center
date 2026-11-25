export type AiEmployeeProduct = {
  id: string;
  title: string;
  description: string;
  category: string;
  priceCents: number;
  billingCycle: "month" | "year" | "one_time";
  version: string;
  skillIds: string[];
  systemPrompt: string;
  allowNetwork: boolean;
  allowedTools: string[];
  applicationIds: string[];
  avatarUrl?: string | null;
  status: "active" | "inactive";
  createdAt: number;
  updatedAt: number;
};

export type PlatformTool = {
  id: string;
  title: string;
  description: string;
  category: string;
  group: string;
  riskLevel: "read" | "write" | "external" | "destructive" | "execution";
};

export type AiEmployeeDeletionEligibility = {
  canDelete: boolean;
  blockers: Array<{
    licenseId: string;
    customerName: string;
    expiresAt: number;
  }>;
};
