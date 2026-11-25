"use client";
import { Button, Input } from "@heroui/react";
import { useState } from "react";
import { ModalShell } from "../../../components/ui/ModalShell";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import type { OperationUser } from "../../../lib/operation-types";

export const roleLabels: Record<string, string> = {
  platform_admin: "平台超级管理员",
  platform_operator: "平台运营",
  platform_finance: "平台财务",
  provider_admin: "服务商管理员",
  provider_sales: "服务商销售",
};

export default function UserModal({
  user,
  currentUserId,
  onClose,
  onSaved,
}: {
  user: OperationUser | null;
  currentUserId?: string;
  onClose: () => void;
  onSaved: () => void;
}) {
  const mutation = useOperationMutation();
  const self = !!user && user.userId === currentUserId;
  const [form, setForm] = useState({
    username: user?.username ?? "",
    displayName: user?.displayName ?? "",
    role: user?.role ?? "platform_operator",
    organizationId: user?.organizationId ?? "platform",
    password: "",
  });
  const change = (key: keyof typeof form, value: string) =>
    setForm((previous) => ({ ...previous, [key]: value }));
  async function save() {
    const body = {
      displayName: form.displayName.trim(),
      role: form.role,
      organizationId: form.organizationId.trim(),
      ...(form.password ? { password: form.password } : {}),
    };
    await mutation.run(
      () =>
        user
          ? operationCommand(
              `/api/users/${encodeURIComponent(user.userId)}`,
              body,
              "PUT",
            )
          : operationCommand("/api/users", {
              ...body,
              username: form.username.trim(),
              password: form.password,
            }),
      onSaved,
    );
  }
  return (
    <ModalShell
      title={user ? "编辑用户" : "新增用户"}
      onClose={() => {
        if (!mutation.pending) onClose();
      }}
    >
      <form
        className="space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <fieldset disabled={mutation.pending} className="space-y-4">
          <label className="flex flex-col gap-1">
            <span>账号</span>
            <Input
              fullWidth
              aria-label="账号"
              value={form.username}
              onChange={(e) => change("username", e.currentTarget.value)}
              disabled={!!user}
              required
              minLength={3}
              maxLength={64}
              pattern="[a-zA-Z0-9_.\-]{3,64}"
              autoComplete="off"
            />
          </label>
          {!user ? (
            <p className="text-xs text-muted">
              3–64 位字母、数字或 _-.，创建后不可修改。
            </p>
          ) : null}
          <label className="flex flex-col gap-1">
            <span>姓名</span>
            <Input
              fullWidth
              aria-label="姓名"
              value={form.displayName}
              onChange={(e) => change("displayName", e.currentTarget.value)}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span>角色</span>
            <select
              aria-label="角色"
              className="w-full rounded-lg border border-border bg-surface p-2"
              disabled={self}
              value={form.role}
              onChange={(e) => change("role", e.currentTarget.value)}
            >
              {Object.entries(roleLabels).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span>所属组织</span>
            <Input
              fullWidth
              aria-label="所属组织"
              disabled={self}
              required
              value={form.organizationId}
              onChange={(e) => change("organizationId", e.currentTarget.value)}
            />
          </label>
          <p className="text-xs text-muted">
            平台账号通常使用 platform；服务商账号填写对应服务商 ID。
          </p>
          <label className="flex flex-col gap-1">
            <span>{user ? "重置密码" : "初始密码"}</span>
            <Input
              fullWidth
              aria-label={user ? "重置密码" : "初始密码"}
              type="password"
              autoComplete="new-password"
              disabled={self}
              required={!user}
              value={form.password}
              onChange={(e) => change("password", e.currentTarget.value)}
            />
          </label>
          {self ? (
            <p className="text-xs text-muted">
              当前账号仅可修改姓名，避免误操作导致无法登录。
            </p>
          ) : user ? (
            <p className="text-xs text-muted">
              密码留空则保留原密码。变更角色、组织或密码后，该用户需重新登录。
            </p>
          ) : null}
        </fieldset>
        {mutation.error ? (
          <p role="alert" className="text-danger">
            {mutation.error}
          </p>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button
            variant="secondary"
            isDisabled={mutation.pending}
            onPress={onClose}
          >
            取消
          </Button>
          <Button type="submit" isPending={mutation.pending}>
            保存
          </Button>
        </div>
      </form>
    </ModalShell>
  );
}
