"use client";
import { useState } from "react";
import { Button, Input } from "@heroui/react";
import { ModalShell } from "../../../../components/ui/ModalShell";
import { useOperationData } from "../../../../hooks/useOperationData";
import { useOperationMutation } from "../../../../hooks/useOperationMutation";
import { useErrorNotification } from "../../../../hooks/useErrorNotification";
import { operationCommand } from "../../../../lib/operation-api";
import type { PlatformInstance } from "../../../../lib/types/platform-delivery";

type History = {
  sessions: {
    id: string;
    username: string;
    reason: string;
    createdAt: number;
    connectedAt: number | null;
    expiresAt: number;
    endedAt: number | null;
  }[];
  events: {
    sessionId: string;
    event: string;
    method: string | null;
    path: string | null;
    createdAt: number;
  }[];
};
export default function InstanceSupportAccess({
  item,
  onClose,
}: {
  item: PlatformInstance;
  onClose: () => void;
}) {
  const path = `/api/platform-instances/${item.instanceId}/support-access`;
  const history = useOperationData<History>(path);
  const mutation = useOperationMutation();
  useErrorNotification(history.error, history.loading);
  useErrorNotification(mutation.error, mutation.pending);
  const [reason, setReason] = useState("");
  const [loginUrl, setLoginUrl] = useState("");
  return (
    <ModalShell title="平台运维访问" width={900} onClose={onClose}>
      <div className="space-y-4">
        <p className="text-sm">
          目标实例：{item.name}
          。仅平台超级管理员可进入，使用独立运维身份并拥有管理员权限；登录与请求保留审计，最长
          30 分钟，运营账号停用或撤权后失效。
        </p>
        <Input
          aria-label="运维访问原因"
          placeholder="请填写本次运维访问原因（4–500 字）"
          maxLength={500}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
        <div className="flex flex-wrap gap-2">
          <Button
            isDisabled={
              mutation.pending ||
              reason.trim().length < 4 ||
              item.status !== "running" ||
              item.licenseSyncStatus !== "synced"
            }
            onPress={() =>
              void mutation.run(async () => {
                setLoginUrl("");
                const result = await operationCommand<{ url: string }>(path, {
                  reason,
                });
                if (result) setLoginUrl(result.url);
              }, history.refresh)
            }
          >
            生成一次性登录入口
          </Button>
          <Button
            variant="danger"
            isDisabled={mutation.pending}
            onPress={() =>
              void mutation.run(async () => {
                await operationCommand(`${path}/revoke`);
                setLoginUrl("");
              }, history.refresh)
            }
          >
            撤销此实例全部运维会话
          </Button>
          <Button variant="secondary" onPress={history.refresh}>
            刷新记录
          </Button>
        </div>
        {loginUrl ? (
          <p className="text-sm">
            <a
              href={loginUrl}
              target="_blank"
              rel="noopener noreferrer"
              referrerPolicy="no-referrer"
              className="text-accent underline"
            >
              进入客户平台
            </a>
            （入口生成后 60 秒内有效，仅可使用一次）
          </p>
        ) : null}
        <h3 className="font-medium">运维访问记录</h3>
        <div className="max-h-56 space-y-2 overflow-auto">
          {history.data?.sessions.map((s) => (
            <div
              key={s.id}
              className="rounded-lg border border-border p-3 text-sm"
            >
              <p>
                {s.username} · {s.reason}
              </p>
              <p className="text-xs text-muted">
                申请：{new Date(s.createdAt).toLocaleString("zh-CN")} · 连接：
                {s.connectedAt
                  ? new Date(s.connectedAt).toLocaleString("zh-CN")
                  : "未连接"}{" "}
                · 结束：
                {s.endedAt
                  ? new Date(s.endedAt).toLocaleString("zh-CN")
                  : `最晚 ${new Date(s.expiresAt).toLocaleString("zh-CN")}`}
              </p>
            </div>
          ))}
          {history.data?.sessions.length === 0 ? (
            <p className="text-sm text-muted">暂无运维访问记录</p>
          ) : null}
        </div>
        <h3 className="font-medium">最近请求审计</h3>
        <div className="max-h-48 overflow-auto text-xs">
          {history.data?.events.map((e, index) => (
            <p key={`${e.sessionId}-${index}`}>
              {new Date(e.createdAt).toLocaleString("zh-CN")} · {e.event}{" "}
              {e.method} {e.path}
            </p>
          ))}
        </div>
      </div>
    </ModalShell>
  );
}
