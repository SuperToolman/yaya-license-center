"use client";
import { useOperation } from "../../../components/OperationProvider";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationData } from "../../../lib/operation-api";
import type {
  AiEmployeeProduct,
  OperationSkill,
  PlatformTool,
} from "../../../lib/operation-types";
export function useAiEmployees() {
  const { refreshToken, refresh } = useOperation();
  const query = useOperationQuery(
    "ai-employees",
    async (signal) => {
      const [employees, skills, tools] = await Promise.all([
        operationData<AiEmployeeProduct[]>("/api/ai-employees", { signal }),
        operationData<OperationSkill[]>("/api/skills", { signal }),
        operationData<PlatformTool[]>("/api/platform-tools", { signal }),
      ]);
      return { employees, skills, tools };
    },
    refreshToken,
  );
  return {
    ...query,
    refresh,
    employees: query.data?.employees ?? [],
    skills: query.data?.skills ?? [],
    tools: query.data?.tools ?? [],
  };
}
