"use client";
import { Plus } from "@gravity-ui/icons";
import { Button, Table } from "@heroui/react";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { useOperation } from "../../../components/OperationProvider";
import { DataPanel } from "../../../components/ui/DataPanel";
import { DataTable } from "../../../components/ui/DataTable";
import { ModalShell } from "../../../components/ui/ModalShell";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import type { OperationUser } from "../../../lib/operation-types";
import UserModal, { roleLabels } from "./UserModal";

type Confirmation = {
  user: OperationUser;
  action: "active" | "inactive" | "delete";
};

export default function UsersPage() {
  const { currentUser } = useOperation();
  const canManage = currentUser?.role === "platform_admin";
  const {
    data: result,
    error: queryError,
    loading,
    refresh,
  } = useData<OperationUser[]>("/api/users");
  const mutation = useOperationMutation();
  const [editing, setEditing] = useState<OperationUser | "new" | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [message, setMessage] = useState("");
  const data = result ?? [];
  const error = mutation.error || queryError;
  function confirm(user: OperationUser, action: Confirmation["action"]) {
    mutation.clearError();
    setMessage("");
    setConfirmation({ user, action });
  }
  async function execute() {
    if (!confirmation) return;
    const { user, action } = confirmation;
    const path = `/api/users/${encodeURIComponent(user.userId)}`;
    await mutation.run(
      () =>
        action === "delete"
          ? operationCommand(path, undefined, "DELETE")
          : operationCommand(`${path}/status`, { status: action }),
      () => {
        setConfirmation(null);
        setMessage(
          action === "delete"
            ? "账号已删除"
            : action === "active"
              ? "账号已启用"
              : "账号已停用，已有会话已失效",
        );
        refresh();
      },
    );
  }
  const confirmationTitle =
    confirmation?.action === "delete"
      ? "删除用户"
      : confirmation?.action === "active"
        ? "启用用户"
        : "停用用户";
  return (
    <>
      <OperationPageHeader
        eyebrow="USER MANAGEMENT"
        title="用户管理"
        description="维护固定角色账号与所属组织。"
        loading={loading || mutation.pending}
        onRefresh={refresh}
        actions={
          canManage ? (
            <Button
              onPress={() => {
                setMessage("");
                setEditing("new");
              }}
            >
              <Plus className="h-4 w-4" />
              新增用户
            </Button>
          ) : undefined
        }
      />
      {error && !confirmation ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {error}
        </p>
      ) : null}
      {message ? (
        <p role="status" className="mb-4 text-sm">
          {message}
        </p>
      ) : null}
      <DataPanel
        title="平台账号"
        note={
          canManage
            ? "仅超级管理员可维护账号；删除前请先停用。"
            : "当前角色仅可查看账号。"
        }
      >
        <DataTable
          label="平台账号"
          empty="尚未创建账号。"
          columns={[
            "账号",
            "姓名",
            "角色",
            "所属组织",
            "状态",
            ...(canManage ? ["操作"] : []),
          ]}
        >
          {data.map((item) => {
            const self = item.userId === currentUser?.userId;
            const lastAdmin =
              item.role === "platform_admin" &&
              item.status === "active" &&
              data.filter(
                (u) => u.role === "platform_admin" && u.status === "active",
              ).length <= 1;
            return (
              <Table.Row key={item.userId} id={item.userId}>
                <Table.Cell>
                  {item.username}
                  {self ? "（当前账号）" : ""}
                </Table.Cell>
                <Table.Cell>{item.displayName || "-"}</Table.Cell>
                <Table.Cell>{roleLabels[item.role] || item.role}</Table.Cell>
                <Table.Cell>{item.organizationId}</Table.Cell>
                <Table.Cell>
                  <StatusChip status={item.status} />
                </Table.Cell>
                {canManage ? (
                  <Table.Cell>
                    <div className="flex gap-2">
                      <Button
                        size="sm"
                        variant="secondary"
                        isDisabled={mutation.pending}
                        onPress={() => {
                          setMessage("");
                          setEditing(item);
                        }}
                      >
                        编辑
                      </Button>
                      <Button
                        size="sm"
                        variant="secondary"
                        isDisabled={self || lastAdmin || mutation.pending}
                        onPress={() =>
                          confirm(
                            item,
                            item.status === "active" ? "inactive" : "active",
                          )
                        }
                      >
                        {item.status === "active" ? "停用" : "启用"}
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        isDisabled={
                          self || item.status !== "inactive" || mutation.pending
                        }
                        onPress={() => confirm(item, "delete")}
                      >
                        删除
                      </Button>
                    </div>
                  </Table.Cell>
                ) : null}
              </Table.Row>
            );
          })}
        </DataTable>
      </DataPanel>
      {editing ? (
        <UserModal
          user={editing === "new" ? null : editing}
          currentUserId={currentUser?.userId}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            setMessage("账号已保存");
            refresh();
          }}
        />
      ) : null}
      {confirmation ? (
        <ModalShell
          title={confirmationTitle}
          onClose={() => {
            if (!mutation.pending) setConfirmation(null);
          }}
        >
          <p className="mb-4 break-all">
            确认{confirmationTitle.slice(0, 2)}账号“{confirmation.user.username}
            ”？
            {confirmation.action === "delete"
              ? "账号与登录会话将被永久删除，业务历史记录保留。此操作无法恢复。"
              : confirmation.action === "inactive"
                ? "停用后该账号将无法登录，已有会话立即失效。"
                : "启用后该账号可重新登录，旧会话不会恢复。"}
          </p>
          {mutation.error ? (
            <p role="alert" className="mb-4 text-danger">
              {mutation.error}
            </p>
          ) : null}
          <div className="flex justify-end gap-2">
            <Button
              variant="secondary"
              isDisabled={mutation.pending}
              onPress={() => setConfirmation(null)}
            >
              取消
            </Button>
            <Button
              variant={confirmation.action === "active" ? "primary" : "danger"}
              isPending={mutation.pending}
              onPress={() => void execute()}
            >
              确认{confirmationTitle.slice(0, 2)}
            </Button>
          </div>
        </ModalShell>
      ) : null}
    </>
  );
}
