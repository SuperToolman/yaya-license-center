"use client";
import { CloudArrowUpIn } from "@gravity-ui/icons";
import { Button, Card, toast } from "@heroui/react";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { StatStrip } from "../../../components/ui/StatStrip";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand, operationRequest } from "../../../lib/operation-api";
import type { OperationSkill } from "../../../lib/operation-types";
import { useSkills } from "../hooks/useSkills";
import SkillModal from "./SkillModal";
import ImportSkillModal from "./ImportSkillModal";

import SkillsTable from "./SkillsTable";
export default function SkillsPage() {
  const { refresh, loading, error: queryError, skills } = useSkills();
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [selected, setSelected] = useState<OperationSkill | null>(null);
  const [importOpen, setImportOpen] = useState(false);
  const submitting = mutation.pending;
  async function archive(file: File, skillId?: string) {
    const body = new FormData();
    body.append("file", file);
    return mutation.run(
      async () => {
        const { response, payload } = await operationRequest<OperationSkill>(
          skillId
            ? `/api/skills/${encodeURIComponent(skillId)}/archive`
            : "/api/skills/import",
          { method: "POST", body },
        );
        if (!response.ok || !payload.data)
          throw new Error(payload.message || "Skill 导入失败");
        return payload.data;
      },
      () => {
        toast.success(skillId ? "Skill ZIP 已更新" : "Skill 已导入");
        refresh();
      },
    );
  }
  async function updateStatus(skill: OperationSkill) {
    await mutation.run(
      () =>
        operationCommand(`/api/skills/${encodeURIComponent(skill.id)}/status`, {
          status: skill.status === "active" ? "inactive" : "active",
        }),
      refresh,
    );
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="SKILL LIBRARY"
        title="Skills 管理"
        description="维护可复用的专业能力、执行说明与工具权限。"
        loading={loading || submitting}
        onRefresh={refresh}
        actions={
          <Button
            onPress={() => {
              mutation.clearError();
              setImportOpen(true);
            }}
            isDisabled={submitting}
          >
            <CloudArrowUpIn className="h-4 w-4" />
            导入
          </Button>
        }
      />
      {error ? (
        <p className="mb-4 text-sm text-danger" role="alert">
          {error}
        </p>
      ) : null}
      <StatStrip
        items={[
          { label: "Skills 总数", value: skills.length },
          {
            label: "启用中",
            value: skills.filter((item) => item.status === "active").length,
          },
        ]}
      />
      <Card>
        <div className="px-4 py-3">
          <h2 className="text-sm font-semibold">Skills 能力库</h2>
        </div>
        <SkillsTable
          skills={skills}
          loading={loading}
          pending={mutation.pending}
          onEdit={setSelected}
          onStatus={(item) => void updateStatus(item)}
          onDeleted={refresh}
        />
      </Card>
      {importOpen ? (
        <ImportSkillModal
          error={mutation.error}
          onClose={() => setImportOpen(false)}
          onImport={async (file) => {
            mutation.clearError();
            const imported = await archive(file);
            if (imported) setImportOpen(false);
          }}
        />
      ) : null}
      {selected ? (
        <SkillModal
          skill={selected}
          submitting={submitting}
          error={error}
          onClose={() => setSelected(null)}
          onSubmit={async (body) => {
            await mutation.run(
              () => operationCommand("/api/skills", body),
              () => {
                setSelected(null);
                refresh();
              },
            );
          }}
          onArchiveUpload={async (file) => {
            if (await archive(file, selected.id)) setSelected(null);
          }}
        />
      ) : null}
    </>
  );
}
