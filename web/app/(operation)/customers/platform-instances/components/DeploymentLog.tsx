"use client";
import { Button } from "@heroui/react";
import { useEffect, useState } from "react";
import { ModalShell } from "../../../../components/ui/ModalShell";
import { useOperationResource } from "../../../../hooks/useOperationQuery";
import type { Deployment } from "../../../../lib/types/platform-delivery";
export default function DeploymentLog({
  id,
  onClose,
  containerId,
  onContainerLogs,
}: {
  id: string;
  onClose: () => void;
  containerId?: string | null;
  onContainerLogs?: (id: string, since: number) => void;
}) {
  const [revision, setRevision] = useState(0);
  const { data, error } = useOperationResource<Deployment[]>(
    `/api/platform-instances/${id}/deployments`,
    revision,
  );
  useEffect(() => {
    const timer = setInterval(() => {
      if (!document.hidden) setRevision((v) => v + 1);
    }, 3000);
    return () => clearInterval(timer);
  }, []);
  const task = data?.[0];
  return (
    <ModalShell title="Docker 部署记录" width={800} onClose={onClose}>
      {error ? <p role="alert">{error}</p> : null}
      {task ? (
        <>
          <p className="mb-3 text-sm">
            {task.stage} · {task.status}
          </p>
          <pre className="max-h-96 overflow-auto whitespace-pre-wrap break-all text-xs">
            {task.logTail}
          </pre>
          {task.error && containerId && onContainerLogs ? (
            <Button
              className="mt-3"
              size="sm"
              onPress={() => onContainerLogs(containerId, task.createdAt)}
            >
              查看失败时段容器日志
            </Button>
          ) : null}
          {task.error && !containerId ? (
            <p className="mt-3 text-xs text-muted">
              容器尚未创建，请根据下方部署错误定位。
            </p>
          ) : null}
          {task.error ? (
            <pre
              role="alert"
              className="mt-3 max-h-64 overflow-auto whitespace-pre-wrap break-all text-xs text-danger"
            >
              {task.error}
            </pre>
          ) : null}
        </>
      ) : (
        <p className="text-sm text-muted">暂无部署记录。</p>
      )}
    </ModalShell>
  );
}
