"use client";
import { CirclePlayFill, CircleStop, Pencil } from "@gravity-ui/icons";
import { Table } from "@heroui/react";
import { IconAction } from "../../../components/ui/IconAction";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { formatDate } from "../../../lib/format";
import type { OperationSkill } from "../../../lib/operation-types";
import DeleteSkillDialog from "./DeleteSkillDialog";
export default function SkillsTable({
  skills,
  loading,
  pending,
  onEdit,
  onStatus,
  onDeleted,
}: {
  skills: OperationSkill[];
  loading: boolean;
  pending: boolean;
  onEdit: (skill: OperationSkill) => void;
  onStatus: (skill: OperationSkill) => void;
  onDeleted: () => void;
}) {
  return (
    <Table>
      <Table.ScrollContainer className="overflow-x-auto">
        <Table.Content aria-label="Skills 能力库" className="min-w-[1040px]">
          <Table.Header>
            {[
              "Skill",
              "来源",
              "能力说明",
              "状态",
              "创建时间",
              "更新时间",
              "操作",
            ].map((column, index) => (
              <Table.Column key={column} id={column} isRowHeader={index === 0}>
                {column}
              </Table.Column>
            ))}
          </Table.Header>
          <Table.Body
            renderEmptyState={() => (
              <div className="p-10 text-center text-sm text-muted">
                {loading ? "正在读取 Skills..." : "暂无 Skill。"}
              </div>
            )}
          >
            {skills.map((skill) => (
              <Table.Row key={skill.id} id={skill.id}>
                <Table.Cell>
                  <strong className="block text-sm">{skill.title}</strong>
                  <small className="text-xs text-muted">
                    {skill.packageName || skill.id} · v{skill.version}
                  </small>
                </Table.Cell>
                <Table.Cell>{skill.source}</Table.Cell>
                <Table.Cell>{skill.description || "通用专业能力"}</Table.Cell>
                <Table.Cell>
                  <StatusChip status={skill.status} />
                </Table.Cell>
                <Table.Cell className="whitespace-nowrap">
                  {formatDate(skill.createdAt)}
                </Table.Cell>
                <Table.Cell className="whitespace-nowrap">
                  {formatDate(skill.updatedAt)}
                </Table.Cell>
                <Table.Cell>
                  <div className="flex gap-1">
                    <IconAction
                      isDisabled={pending || loading}
                      label={`编辑 ${skill.title}`}

                      onPress={() => onEdit(skill)}
                    >
                      <Pencil />
                    </IconAction>
                    <IconAction
                      isDisabled={pending || loading}
                      label={
                        skill.status === "active" ? "停用 Skill" : "启用 Skill"
                      }

                      onPress={() => onStatus(skill)}
                    >
                      {skill.status === "active" ? (
                        <CircleStop />
                      ) : (
                        <CirclePlayFill />
                      )}
                    </IconAction>
                    <DeleteSkillDialog skill={skill} onDeleted={onDeleted} />
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
