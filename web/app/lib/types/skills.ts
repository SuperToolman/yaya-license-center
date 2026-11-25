export type OperationSkill = {
  id: string;
  title: string;
  packageName: string;
  source: string;
  version: string;
  packagePath: string;
  description: string;
  instructions: string;
  environmentVariables?: Array<{ name: string; secret: boolean }>;
  environmentValues?: Record<string, string>;
  status: "active" | "inactive";
  createdAt: number;
  updatedAt: number;
};

export type SkillDeletionEligibility = {
  canDelete: boolean;
  blockers: Array<{
    employeeId: string;
    employeeTitle: string;
    employeeStatus: string;
  }>;
};
