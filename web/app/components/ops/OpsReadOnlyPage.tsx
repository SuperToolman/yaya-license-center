"use client";

import { Card, Input, Table } from "@heroui/react";
import { useEffect, useState } from "react";
import OperationPageHeader from "../OperationPageHeader";
import { DataPanel } from "../ui/DataPanel";
import { DataTable } from "../ui/DataTable";
import { StatStrip } from "../ui/StatStrip";
import { useOperationQuery } from "../../hooks/useOperationQuery";
import { operationData } from "../../lib/operation-api";
import type { OpsContainers, OpsMetrics } from "../../lib/types/ops";
import { useOps } from "./OpsProvider";

type PageKind = "overview" | "nodes" | "containers" | "gpu";
const titles: Record<PageKind, string> = {
  overview: "运维总览",
  nodes: "主机节点",
  containers: "容器管理",
  gpu: "GPU 监控",
};
const reasons: Record<string, string> = {
  no_device: "未检测到 NVIDIA GPU",
  permission_denied: "当前采集账户无权读取 GPU",
  driver_unavailable: "NVIDIA 驱动不可用",
  library_or_driver_unavailable: "NVML 库或 NVIDIA 驱动不可用",
  device_query_failed: "GPU 设备查询失败",
  linux_required:
    "当前系统环境不支持查看 GPU 数据，此功能需要 Linux 宿主机环境",
};
function bytes(value: number | null | undefined) {
  return value == null ? "不可用" : `${(value / 1024 ** 3).toFixed(1)} GiB`;
}
function number(value: number | null | undefined, suffix: string) {
  return value == null ? "不可用" : `${value.toFixed(1)}${suffix}`;
}
function date(value: number) {
  return new Date(value).toLocaleString("zh-CN");
}

export default function OpsReadOnlyPage({ kind }: { kind: PageKind }) {
  const { node, nodes, revision, refresh } = useOps();
  const [query, setQuery] = useState("");
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 5_000);
    return () => window.clearInterval(timer);
  }, []);
  const online = node?.status === "online";
  const loadMetrics = online && (kind === "overview" || kind === "gpu");
  const loadContainers = online && kind === "containers";
  const metrics = useOperationQuery<OpsMetrics | null>(
    `ops-metrics:${loadMetrics ? node.nodeId : "none"}`,
    (signal) =>
      loadMetrics
        ? operationData(
            `/api/ops/nodes/${encodeURIComponent(node.nodeId)}/metrics/latest`,
            { signal },
          )
        : Promise.resolve(null),
    revision,
  );
  const containers = useOperationQuery<OpsContainers | null>(
    `ops-containers:${loadContainers ? node.nodeId : "none"}`,
    (signal) =>
      loadContainers
        ? operationData(
            `/api/ops/nodes/${encodeURIComponent(node.nodeId)}/containers`,
            { signal },
          )
        : Promise.resolve(null),
    revision,
  );
  const snapshot = metrics.data;
  const stale = Boolean(
    snapshot && (now - snapshot.capturedAtMs > 15_000 || metrics.error),
  );
  const visible =
    containers.data?.items.filter((item) =>
      [item.name, item.image, item.id]
        .join(" ")
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
    ) ?? [];
  return (
    <>
      <OperationPageHeader
        eyebrow="YAYA OPERATIONS"
        title={titles[kind]}
        description="查看本机资源与运行状态。当前阶段仅提供只读能力。"
        loading={metrics.loading || containers.loading}
        onRefresh={refresh}
      />
      {metrics.error || containers.error ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {metrics.error || containers.error}
        </p>
      ) : null}
      {stale ? (
        <p role="status" className="mb-4 text-sm text-warning">
          数据已过期，以下为最近一次成功采样。
        </p>
      ) : null}
      {!node ? (
        <DataPanel title="资源监控暂不可用" note="当前无法获取本机资源数据。">
          <p className="px-4 pb-4 text-sm text-muted">
            本机资源监控服务暂不可用，请稍后刷新重试。
          </p>
        </DataPanel>
      ) : !online && kind !== "nodes" ? (
        <p role="status" className="text-sm text-muted">
          节点离线，无法获取当前资源数据。最近在线时间：
          {date(node.lastSeenAtMs)}
        </p>
      ) : null}
      {kind === "nodes" ? (
        <DataPanel
          title="已接入节点"
          note="显示当前环境的主机信息与资源监控状态。"
        >
          <DataTable
            label="主机节点"
            columns={[
              "节点",
              "状态",
              "监控服务版本",
              "Docker",
              "GPU",
              "最近在线",
            ]}
            empty="尚未接入节点"
          >
            {nodes?.items.map((item) => (
              <Table.Row key={item.nodeId} id={item.nodeId}>
                <Table.Cell>
                  <strong>{item.name}</strong>
                  <p className="mt-1 text-xs text-muted">{item.nodeId}</p>
                </Table.Cell>
                <Table.Cell>
                  {item.status === "online" ? "在线" : "离线"}
                </Table.Cell>
                <Table.Cell>{item.agentVersion}</Table.Cell>
                <Table.Cell>
                  {item.docker.available ? "可用" : "不可用"}
                </Table.Cell>
                <Table.Cell>
                  {item.nvidia.available ? "可用" : "不可用"}
                </Table.Cell>
                <Table.Cell>{date(item.lastSeenAtMs)}</Table.Cell>
              </Table.Row>
            ))}
          </DataTable>
        </DataPanel>
      ) : null}
      {kind === "overview" && online ? (
        <>
          <StatStrip
            items={[
              {
                label: "主机 CPU",
                value: number(snapshot?.host?.cpuPercent, "%"),
              },
              {
                label: "已用内存",
                value: bytes(snapshot?.host?.memoryUsedBytes),
              },
              {
                label: "总内存",
                value: bytes(snapshot?.host?.memoryTotalBytes),
              },
              {
                label: "GPU 数量",
                value: snapshot
                  ? snapshot.gpuCapability.available
                    ? snapshot.gpus.length
                    : "不可用"
                  : "加载中",
              },
            ]}
          />
          <DataPanel
            title={node.name}
            note={`节点状态：在线 · 监控服务 ${node.agentVersion}`}
          >
            <dl className="grid gap-4 px-4 pb-4 text-sm sm:grid-cols-2">
              <div>
                <dt className="text-muted">逻辑 CPU</dt>
                <dd>{snapshot?.host?.logicalCpus ?? "不可用"}</dd>
              </div>
              <div>
                <dt className="text-muted">运行时间</dt>
                <dd>
                  {snapshot?.host
                    ? `${Math.floor(snapshot.host.uptimeSeconds / 3600)} 小时`
                    : "不可用"}
                </dd>
              </div>
              <div>
                <dt className="text-muted">Docker</dt>
                <dd>
                  {node.docker.available ? "已连接" : "不可用或版本不受支持"}
                </dd>
              </div>
              <div>
                <dt className="text-muted">采样时间</dt>
                <dd>{snapshot ? date(snapshot.capturedAtMs) : "等待采样"}</dd>
              </div>
            </dl>
          </DataPanel>
        </>
      ) : null}
      {kind === "containers" && online ? (
        <DataPanel
          title="Docker 容器"
          note={
            containers.data
              ? `显示 ${visible.length} 个容器${containers.data.truncated ? `，已达到 ${containers.data.limit} 个展示上限` : ""}`
              : "正在读取容器列表"
          }
        >
          <div className="px-4 pb-4">
            <Input
              aria-label="搜索容器"
              placeholder="搜索容器名称、镜像或 ID"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>
          <DataTable
            label="Docker 容器"
            columns={["容器", "镜像", "状态", "运行信息", "创建时间"]}
            empty={
              containers.error
                ? "容器数据读取失败"
                : containers.loading
                  ? "正在读取容器"
                  : "没有符合条件的容器"
            }
          >
            {visible.map((item) => (
              <Table.Row key={item.id} id={item.id}>
                <Table.Cell>
                  <strong>{item.name}</strong>
                  <p className="mt-1 text-xs text-muted">
                    {item.id.slice(0, 12)}
                  </p>
                </Table.Cell>
                <Table.Cell>{item.image}</Table.Cell>
                <Table.Cell>{item.state}</Table.Cell>
                <Table.Cell>{item.status}</Table.Cell>
                <Table.Cell>{date(item.createdAtMs)}</Table.Cell>
              </Table.Row>
            ))}
          </DataTable>
        </DataPanel>
      ) : null}
      {kind === "gpu" && online ? (
        <>
          {snapshot && !snapshot.gpuCapability.available ? (
            <p role="status" className="mb-4 text-sm text-muted">
              {reasons[snapshot.gpuCapability.reason ?? ""] ??
                "GPU 指标暂不可用"}
            </p>
          ) : null}
          <div className="grid gap-4 xl:grid-cols-2">
            {snapshot?.gpus.map((gpu) => (
              <Card key={gpu.uuid} className="p-5">
                <h2 className="font-semibold">{gpu.name}</h2>
                <p className="mt-1 break-all text-xs text-muted">{gpu.uuid}</p>
                <dl className="mt-5 grid grid-cols-2 gap-5 text-sm">
                  <div>
                    <dt className="text-muted">GPU 利用率</dt>
                    <dd>{number(gpu.utilizationPercent, "%")}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">显存使用</dt>
                    <dd>
                      {bytes(gpu.memoryUsedBytes)} /{" "}
                      {bytes(gpu.memoryTotalBytes)}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-muted">温度</dt>
                    <dd>{number(gpu.temperatureCelsius, " °C")}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">功耗</dt>
                    <dd>{number(gpu.powerWatts, " W")}</dd>
                  </div>
                </dl>
                <p className="mt-5 text-xs text-muted">
                  采样时间：{date(snapshot.capturedAtMs)}
                </p>
              </Card>
            ))}
          </div>
        </>
      ) : null}
    </>
  );
}
