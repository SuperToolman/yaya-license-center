"use client";
import {
  CirclePlayFill,
  CircleStop,
  FaceRobot,
  Pencil,
} from "@gravity-ui/icons";
import { Avatar, Table } from "@heroui/react";
import { IconAction } from "../../../components/ui/IconAction";
import { SemanticChip, StatusChip } from "../../../components/ui/SemanticChip";
import type {
  AiEmployeeProduct,
  OperationSkill,
} from "../../../lib/operation-types";
import DeleteEmployee from "./DeleteEmployee";
export default function EmployeesTable({
  employees,
  skills,
  loading,
  pending,
  onEdit,
  onStatus,
  onDeleted,
}: {
  employees: AiEmployeeProduct[];
  skills: OperationSkill[];
  loading: boolean;
  pending: boolean;
  onEdit: (employee: AiEmployeeProduct) => void;
  onStatus: (employee: AiEmployeeProduct) => void;
  onDeleted: () => void;
}) {
  return (
    <Table>
      <Table.ScrollContainer className="overflow-x-auto">
        <Table.Content aria-label="AI 员工模板" className="min-w-[860px]">
          <Table.Header>
            {["AI 员工", "Skills", "价格", "计费", "状态", "操作"].map(
              (column, index) => (
                <Table.Column
                  key={column}
                  id={column}
                  isRowHeader={index === 0}
                >
                  {column}
                </Table.Column>
              ),
            )}
          </Table.Header>
          <Table.Body
            renderEmptyState={() => (
              <div className="p-10 text-center text-sm text-muted">
                {loading ? "正在读取 AI 员工..." : "暂无 AI 员工模板。"}
              </div>
            )}
          >
            {employees.map((employee) => (
              <Table.Row key={employee.id} id={employee.id}>
                <Table.Cell>
                  <div className="flex items-center gap-3">
                    <EmployeeAvatar employee={employee} />
                    <div>
                      <strong className="block text-sm">
                        {employee.title}
                      </strong>
                      <small className="text-xs text-muted">
                        {employee.id} · 员工 v{employee.version}
                      </small>
                    </div>
                  </div>
                </Table.Cell>
                <Table.Cell>
                  {employee.skillIds.length
                    ? employee.skillIds.map((id) => (
                        <SemanticChip key={id} color="default" className="mr-1">
                          {skills.find((skill) => skill.id === id)?.title || id}
                        </SemanticChip>
                      ))
                    : "未配置"}
                </Table.Cell>
                <Table.Cell>
                  ¥{(employee.priceCents / 100).toFixed(2)}
                </Table.Cell>
                <Table.Cell>
                  {
                    { month: "按月", year: "按年", one_time: "一次性" }[
                      employee.billingCycle
                    ]
                  }
                </Table.Cell>
                <Table.Cell>
                  <StatusChip
                    status={employee.status}
                    label={
                      employee.status === "active"
                        ? "上架"
                        : employee.status === "inactive"
                          ? "下架"
                          : undefined
                    }
                  />
                </Table.Cell>
                <Table.Cell>
                  <div className="flex gap-1">
                    <IconAction
                      isDisabled={pending || loading}
                      label="编辑"

                      onPress={() => onEdit(employee)}
                    >
                      <Pencil />
                    </IconAction>
                    <IconAction
                      isDisabled={pending || loading}
                      label={employee.status === "active" ? "下架" : "上架"}

                      onPress={() => onStatus(employee)}
                    >
                      {employee.status === "active" ? (
                        <CircleStop />
                      ) : (
                        <CirclePlayFill />
                      )}
                    </IconAction>
                    <DeleteEmployee employee={employee} onDeleted={onDeleted} />
                  </div>
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
function EmployeeAvatar({ employee }: { employee: AiEmployeeProduct }) {
  return (
    <Avatar size="sm" color="accent" variant="soft">
      {employee.avatarUrl ? (
        <Avatar.Image
          src={`/operation-api/api/ai-employees/${encodeURIComponent(employee.id)}/avatar?v=${encodeURIComponent(employee.avatarUrl)}`}
          alt=""
        />
      ) : null}
      <Avatar.Fallback>
        <FaceRobot className="h-5 w-5" />
      </Avatar.Fallback>
    </Avatar>
  );
}
