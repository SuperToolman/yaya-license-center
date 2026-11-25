"use client";
import { Plus } from "@gravity-ui/icons";
import { Button, Card } from "@heroui/react";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { StatStrip } from "../../../components/ui/StatStrip";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand, operationData } from "../../../lib/operation-api";
import type { AiEmployeeProduct } from "../../../lib/operation-types";
import { useAiEmployees } from "../hooks/useAiEmployees";
import EmployeeModal from "./EmployeeModal";
import EmployeesTable from "./EmployeesTable";
export default function AiEmployeesPage() {
  const {
    refresh,
    loading,
    error: queryError,
    employees,
    skills,
    tools,
  } = useAiEmployees();
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [selected, setSelected] = useState<
    AiEmployeeProduct | null | undefined
  >(undefined);
  const submitting = mutation.pending;
  async function status(employee: AiEmployeeProduct) {
    await mutation.run(
      () =>
        operationCommand(
          `/api/ai-employees/${encodeURIComponent(employee.id)}/status`,
          { status: employee.status === "active" ? "inactive" : "active" },
        ),
      refresh,
    );
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="AI WORKFORCE"
        title="AI 员工管理"
        description="维护可销售、可授权的 AI 员工模板。"
        loading={loading || submitting}
        onRefresh={refresh}
        actions={
          <Button onPress={() => setSelected(null)}>
            <Plus className="h-4 w-4" />
            新增 AI 员工
          </Button>
        }
      />
      {error ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {error}
        </p>
      ) : null}
      <StatStrip
        items={[
          { label: "AI 员工模板", value: employees.length },
          {
            label: "销售中",
            value: employees.filter((item) => item.status === "active").length,
          },
          {
            label: "已封装 Skills",
            value: new Set(employees.flatMap((item) => item.skillIds)).size,
          },
          {
            label: "商品分类",
            value: new Set(employees.map((item) => item.category)).size,
          },
        ]}
      />
      <Card>
        <div className="px-4 py-3">
          <h2 className="text-sm font-semibold">AI 员工模板目录</h2>
        </div>
        <EmployeesTable
          employees={employees}
          skills={skills}
          loading={loading}
          pending={mutation.pending}
          onEdit={setSelected}
          onStatus={(item) => void status(item)}
          onDeleted={refresh}
        />
      </Card>
      {selected !== undefined ? (
        <EmployeeModal
          employee={selected}
          skills={skills}
          tools={tools}
          submitting={submitting}
          error={error}
          onClose={() => setSelected(undefined)}
          onSubmit={async (body, avatar) => {
            await mutation.run(
              async () => {
                const saved = await operationCommand<AiEmployeeProduct>(
                  "/api/ai-employees",
                  {
                    ...body,
                    id: selected?.id || body.id,
                  },
                );
                if (!saved?.id)
                  throw new Error("AI 员工保存响应无效，请刷新后重试。");
                // Keep the created ID if the avatar upload fails so retries update the same employee.
                setSelected(saved);
                refresh();
                if (avatar) {
                  try {
                    await operationData<AiEmployeeProduct>(
                      `/api/ai-employees/${encodeURIComponent(saved.id)}/avatar`,
                      {
                        method: "PUT",
                        headers: { "content-type": avatar.type },
                        body: avatar,
                      },
                    );
                  } catch (cause) {
                    throw new Error(
                      `AI 员工信息已保存，但头像上传失败：${cause instanceof Error ? cause.message : "请重试"}。可再次点击保存重试。`,
                    );
                  }
                }
              },
              () => {
                setSelected(undefined);
                refresh();
              },
            );
          }}
        />
      ) : null}
    </>
  );
}
