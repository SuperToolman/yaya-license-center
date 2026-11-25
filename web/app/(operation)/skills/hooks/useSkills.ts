"use client";
import { useOperation } from "../../../components/OperationProvider";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationData } from "../../../lib/operation-api";
import type { OperationSkill } from "../../../lib/operation-types";
export function useSkills() {
  const { refreshToken, refresh } = useOperation();
  const query = useOperationQuery(
    "skills",
    async (signal) => {
      const [skills] = await Promise.all([
        operationData<OperationSkill[]>("/api/skills", { signal }),
      ]);
      return { skills };
    },
    refreshToken,
  );
  return { ...query, refresh, skills: query.data?.skills ?? [] };
}
