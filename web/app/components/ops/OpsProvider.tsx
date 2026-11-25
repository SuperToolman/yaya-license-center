"use client";

import { usePathname } from "next/navigation";
import OperationPageHeader from "../OperationPageHeader";
import { Button } from "@heroui/react";
import { createContext, useContext, useEffect, useState } from "react";
import { useOperationResource } from "../../hooks/useOperationQuery";
import type { OpsContext, OpsNode, OpsNodes } from "../../lib/types/ops";

type OpsValue = {
  node: OpsNode | null;
  nodes: OpsNodes | null;
  selectNode: (id: string) => void;
  refresh: () => void;
  revision: number;
};
const Context = createContext<OpsValue | null>(null);
const SELECTION_KEY = "yaya-ops-ui-v1";

export function OpsProvider({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  if (
    pathname === "/ops/files" ||
    pathname === "/ops/containers" ||
    pathname.startsWith("/ops/containers/")
  )
    return <>{children}</>;
  return <HostOpsProvider>{children}</HostOpsProvider>;
}

function HostOpsProvider({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const resource = pathname.endsWith("/gpu")
    ? "GPU 监控"
    : pathname.endsWith("/containers")
      ? "容器监控"
      : pathname.endsWith("/nodes")
        ? "主机节点信息"
        : "资源总览";
  const context = useOperationResource<OpsContext>("/api/ops/context");
  if (context.loading && !context.data)
    return <p className="text-sm text-muted">正在检查运维访问权限…</p>;
  if (context.error)
    return (
      <div role="alert" className="space-y-3">
        <p>{context.error}</p>
        <Button variant="secondary" onPress={context.refresh}>
          重试
        </Button>
      </div>
    );
  if (
    !context.data?.allowed ||
    context.data.environmentSupported === false ||
    !context.data.enabled
  ) {
    const denied = !context.data?.allowed;
    const unsupported = context.data?.environmentSupported === false;
    return (
      <>
        <OperationPageHeader
          eyebrow="RESOURCE MONITORING"
          title={resource}
          description="查看当前环境的资源与运行状态。"
          loading={false}
          onRefresh={context.refresh}
        />
        <p role="status" className="text-sm text-muted">
          {denied
            ? "当前账号没有资源监控访问权限。"
            : unsupported
              ? `当前系统环境不支持查看${resource}，此功能目前仅支持 Linux 宿主机环境。`
              : `当前环境尚未启用资源监控，暂时无法查看${resource}。`}
        </p>
      </>
    );
  }
  return <ConnectedOps>{children}</ConnectedOps>;
}

function ConnectedOps({ children }: { children: React.ReactNode }) {
  const [revision, setRevision] = useState(0);
  const [selected, setSelected] = useState("");
  const nodes = useOperationResource<OpsNodes>("/api/ops/nodes", revision);
  useEffect(() => {
    // Storage is a preference only. The selected ID is validated against current server visibility below.
    try {
      const saved: unknown = JSON.parse(
        localStorage.getItem(SELECTION_KEY) ?? "{}",
      );
      if (
        saved &&
        typeof saved === "object" &&
        "nodeId" in saved &&
        typeof saved.nodeId === "string"
      ) {
        const id = saved.nodeId;
        void Promise.resolve().then(() => setSelected(id));
      }
    } catch {
      /* Storage is optional. */
    }
  }, []);
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible")
        setRevision((value) => value + 1);
    }, 10_000);
    return () => window.clearInterval(timer);
  }, []);
  const node =
    nodes.data?.items.find((item) => item.nodeId === selected) ??
    nodes.data?.items[0] ??
    null;
  function selectNode(id: string) {
    if (!nodes.data?.items.some((item) => item.nodeId === id)) return;
    setSelected(id);
    try {
      localStorage.setItem(SELECTION_KEY, JSON.stringify({ nodeId: id }));
    } catch {
      /* Storage is optional. */
    }
  }
  return (
    <Context.Provider
      value={{
        node,
        nodes: nodes.data,
        selectNode,
        revision,
        refresh: () => setRevision((value) => value + 1),
      }}
    >
      <div className="mb-6 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-surface p-3">
        <label className="flex items-center gap-3 text-sm">
          当前节点
          <select
            aria-label="当前节点"
            className="max-w-[240px] rounded-md border border-border bg-surface px-3 py-2"
            value={node?.nodeId ?? ""}
            onChange={(event) => selectNode(event.target.value)}
            disabled={!nodes.data?.items.length}
          >
            {!nodes.data?.items.length ? (
              <option value="">尚未接入节点</option>
            ) : null}
            {nodes.data?.items.map((item) => (
              <option key={item.nodeId} value={item.nodeId}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <span className="text-xs text-muted">
          只读监控 · {node?.status === "online" ? "节点在线" : "节点未连接"}
        </span>
      </div>
      {nodes.error || nodes.data?.connectionError ? (
        <div
          role="alert"
          className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning p-4 text-sm"
        >
          <p>{nodes.error || nodes.data?.connectionError}</p>
          <Button
            variant="secondary"
            size="sm"
            onPress={() => setRevision((value) => value + 1)}
          >
            重新连接
          </Button>
        </div>
      ) : null}
      {nodes.loading && !nodes.data ? (
        <p className="mb-4 text-sm text-muted">正在连接资源监控服务…</p>
      ) : null}
      {children}
    </Context.Provider>
  );
}

export function useOps() {
  const value = useContext(Context);
  if (!value) throw new Error("useOps requires OpsProvider");
  return value;
}
