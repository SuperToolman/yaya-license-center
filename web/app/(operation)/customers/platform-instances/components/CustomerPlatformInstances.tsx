"use client";
import { Button, Chip, Input } from "@heroui/react";
import { useCustomerAuthorization } from "../../../../components/CustomerAuthorizationPanel";
import { useState, useEffect } from "react";
import { Gear } from "@gravity-ui/icons";
import Link from "next/link";
import { containerSearchHref } from "../../../../components/ops/docker-routes";
import InstanceActions from "./InstanceActions";
import ContainerDetails from "../../../../components/ops/ContainerDetails";

import { ModalShell } from "../../../../components/ui/ModalShell";
import { useOperationData } from "../../../../hooks/useOperationData";
import { useOperationMutation } from "../../../../hooks/useOperationMutation";
import { operationCommand, operationUrl } from "../../../../lib/operation-api";
import {
  instanceStatusLabel,
  instanceRuntimeLabel,
  type DockerRuntime,
  type InstanceOptions,
  type PlatformInstance,
} from "../../../../lib/types/platform-delivery";
import InstanceEditor from "./InstanceEditor";
import InstanceSupportAccess from "./InstanceSupportAccess";
import InstanceVersions from "./InstanceVersions";
import DeploymentLog from "./DeploymentLog";
import DeploymentSettings from "./DeploymentSettings";

const statusColors: Record<
  PlatformInstance["status"],
  "warning" | "accent" | "success" | "danger" | "default"
> = {
  not_deployed: "warning",
  deploying: "accent",
  running: "success",
  failed: "danger",
  archived: "default",
};

function instanceAddress(item: PlatformInstance, origin: string): string {
  if (item.publicUrl) return item.publicUrl;
  if (!origin) return "";
  const url = new URL(origin);
  url.protocol = "http:";
  url.port = String(item.webPort);
  return url.origin;
}

export default function CustomerPlatformInstances({
  customer,
}: {
  customer: import("../../../../lib/operation-types").Customer;
}) {
  const customerId = customer.customerId;
  const [origin, setOrigin] = useState("");
  useEffect(() => setOrigin(window.location.origin), []);
  const authorization = useCustomerAuthorization(customerId);
  const latest = authorization.data?.latest;
  const canDeploy =
    !!latest &&
    latest.platformStatus !== "destroyed" &&
    latest.platformStatus !== "expired";
  const query = useOperationData<PlatformInstance[]>(
    `/api/customers/${encodeURIComponent(customerId)}/platform-instances`,
  );
  const options = useOperationData<InstanceOptions>(
    "/api/platform-instances/options",
  );
  const runtime = useOperationData<DockerRuntime>("/api/platform-runtime");
  const [container, setContainer] = useState<{
    id: string;
    logs: boolean;
    terminal?: boolean;
    since?: number;
  } | null>(null);
  const [logId, setLogId] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [supportInstance, setSupportInstance] =
    useState<PlatformInstance | null>(null);
  const refresh = query.refresh;
  const refreshRuntime = runtime.refresh;
  useEffect(() => {
    const timer = setInterval(() => {
      if (!document.hidden) {
        refresh();
        refreshRuntime();
      }
    }, 5000);
    return () => clearInterval(timer);
  }, [refresh, refreshRuntime]);
  const mutation = useOperationMutation();
  const desktopDownload = useOperationMutation();
  const [search, setSearch] = useState("");
  const [editor, setEditor] = useState<{
    initial: PlatformInstance | null;
  } | null>(null);
  const [selectedDetail, setDetail] = useState<PlatformInstance | null>(null);
  const detail =
    query.data?.find((i) => i.instanceId === selectedDetail?.instanceId) ??
    selectedDetail;
  const [versionInstance, setVersionInstance] =
    useState<PlatformInstance | null>(null);
  const [archive, setArchive] = useState<PlatformInstance | null>(null);
  const items = (query.data ?? []).filter((item) =>
    `${item.name} ${item.customerName} ${item.version} ${item.packageId} ${item.containerName} ${item.publicUrl} ${item.instanceId}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  function downloadDesktopInstaller(item: PlatformInstance) {
    void desktopDownload.run(async () => {
      const path = `/api/platform-instances/${encodeURIComponent(item.instanceId)}/desktop-installer`;
      const response = await fetch(operationUrl(path), {
        method: "POST",
        credentials: "include",
      });
      if (!response.ok) {
        const payload = await response.json().catch(() => null);
        throw new Error(payload?.message || `生成安装包失败（HTTP ${response.status}）`);
      }
      if (response.headers.get("content-type") !== "application/octet-stream") {
        throw new Error("运营服务返回了无效的安装包，请稍后重试。");
      }
      const link = document.createElement("a");
      const url = URL.createObjectURL(await response.blob());
      link.href = url;
      link.download = "yaya低代码平台.exe";
      link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 30_000);
    });
  }
  async function confirmArchive() {
    if (!archive) return;
    await mutation.run(
      () =>
        operationCommand(
          `/api/platform-instances/${archive.instanceId}/archive`,
        ),
      () => {
        setArchive(null);
        query.refresh();
      },
    );
  }
  return (
    <>
      <div className="mb-4 flex flex-wrap items-center justify-between gap-x-4 gap-y-3 border-b border-black/10 pb-4 dark:border-white/10">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1">
          <h3 className="shrink-0 text-sm font-semibold">
            平台实例{" "}
            <span className="ml-1 font-normal text-muted">
              {query.data?.length ?? 0}
            </span>
          </h3>
          <span
            role={!canDeploy ? "status" : undefined}
            className="text-xs text-muted"
          >
            {authorization.error ||
              (authorization.loading
                ? "正在读取客户授权…"
                : !canDeploy
                  ? "暂无有效授权 · 请先在订单中完成回款与授权交付"
                  : `客户授权第 ${authorization.data?.revision} 版 · 权益变更后可同步授权`)}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Input
            aria-label="搜索平台实例"
            placeholder="搜索实例、版本或容器"
            className="w-56 max-w-full"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <div className="flex items-center gap-2">
            <Button variant="secondary" onPress={query.refresh}>
              刷新实例
            </Button>{" "}
            <Button
              isIconOnly
              variant="secondary"
              aria-label="部署设置"
              onPress={() => setSettingsOpen(true)}
            >
              <Gear className="h-4 w-4" />
            </Button>
            <Button
              isDisabled={
                !options.data ||
                !!options.error ||
                !runtime.data?.available ||
                customer.status !== "active" ||
                !canDeploy ||
                !!authorization.error
              }
              onPress={() => setEditor({ initial: null })}
            >
              新建实例
            </Button>
          </div>
        </div>
      </div>
      {!runtime.data?.available ? (
        <div role="status" className="mb-4 rounded-lg bg-default p-4 text-sm">
          {runtime.error ||
            runtime.data?.message ||
            "正在检测当前 Docker 环境…"}
          。Docker 可用后即可新建并部署平台实例。
        </div>
      ) : null}
      {desktopDownload.pending ? (
        <p role="status" className="mb-3 text-sm text-muted">正在生成桌面端安装包…</p>
      ) : null}
      {query.error || options.error || mutation.error || desktopDownload.error ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {desktopDownload.error || query.error || options.error || mutation.error}
        </p>
      ) : null}
      <section aria-label="客户平台实例" className="min-w-0 space-y-3">
        <ul
          aria-label="平台实例"
          className="m-0 min-w-0 list-none space-y-3 p-0"
        >
          {items.map((item) => (
            <li
              key={item.instanceId}
              className="min-w-0 rounded-xl border border-black/10 bg-white p-4 dark:border-white/10 dark:bg-zinc-900"
            >
              <div className="flex items-start justify-between gap-4">
                <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1">
                  <h4 className="break-words text-sm font-semibold leading-6 text-foreground [overflow-wrap:anywhere]">
                    {item.name}
                  </h4>
                  <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                    <Chip size="sm" variant="soft">
                      版本 {item.version}
                    </Chip>
                    <div className="flex flex-wrap items-center gap-2">
                      <span data-instance-status={item.status}>
                        <Chip
                          size="sm"
                          variant="soft"
                          color={
                            item.containerId
                              ? item.runtime?.state === "running"
                                ? "success"
                                : item.runtime?.state === "exited"
                                  ? "warning"
                                  : "default"
                              : statusColors[item.status]
                          }
                        >
                          {instanceRuntimeLabel(item)}
                        </Chip>
                      </span>
                      {item.runtime?.checkedAt ? (
                        <span className="whitespace-nowrap text-xs text-muted">
                          核对于{" "}
                          {new Date(item.runtime.checkedAt).toLocaleTimeString(
                            "zh-CN",
                          )}
                        </span>
                      ) : null}
                    </div>
                    {item.runtime?.message ? (
                      <p className="mt-1 text-xs text-muted">
                        {item.runtime.message}
                      </p>
                    ) : null}
                  </div>
                </div>
                <div className="shrink-0">
                  {" "}
                  <InstanceActions
                    item={item}
                    busy={!runtime.data?.available || mutation.pending}
                    canEdit={!!options.data}
                    onDetail={() => setDetail(item)}
                    onVersions={() => setVersionInstance(item)}
                    onSupport={() => setSupportInstance(item)}
                    onDesktopDownload={() => downloadDesktopInstaller(item)}
                    desktopDownloading={desktopDownload.pending}
                    onTerminal={() =>
                      item.containerId &&
                      setContainer({
                        id: item.containerId,
                        logs: false,
                        terminal: true,
                      })
                    }
                    onContainer={(logs) =>
                      item.containerId &&
                      setContainer({ id: item.containerId, logs })
                    }
                    onLogs={() => setLogId(item.instanceId)}
                    onDeploy={() => {
                      void mutation.run(
                        () =>
                          operationCommand(
                            `/api/platform-instances/${item.instanceId}/deploy`,
                          ),
                        () => {
                          query.refresh();
                          setLogId(item.instanceId);
                        },
                      );
                    }}
                    onEdit={() => setEditor({ initial: item })}
                    onArchive={() => {
                      mutation.clearError();
                      setArchive(item);
                    }}
                  />
                </div>
              </div>
              <div className="mt-3 grid min-w-0 grid-cols-1 gap-x-8 gap-y-2 text-sm leading-6 md:grid-cols-2">
                <div className="flex min-w-0 items-start gap-3">
                  <p className="w-20 shrink-0 text-sm leading-6 text-muted">
                    访问地址
                  </p>
                  {/^https?:\/\//i.test(instanceAddress(item, origin)) ? (
                    <a
                      href={instanceAddress(item, origin)}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="min-w-0 break-all text-sm text-accent underline-offset-4 hover:underline dark:text-blue-400"
                    >
                      {instanceAddress(item, origin)}
                    </a>
                  ) : (
                    <p className="break-all text-sm text-muted">
                      {item.publicUrl || "未配置访问地址"}
                    </p>
                  )}
                </div>
                <div className="flex min-w-0 flex-wrap items-start gap-3">
                  <p className="w-20 shrink-0 text-sm leading-6 text-muted">
                    授权同步
                  </p>
                  <p className="whitespace-nowrap text-sm">
                    {item.licenseSyncStatus === "syncing"
                      ? "同步中"
                      : item.licenseSyncStatus === "failed"
                        ? "同步失败"
                        : item.licenseSyncStatus === "interrupted"
                          ? "同步中断"
                          : !item.authorizationId
                            ? "未绑定授权"
                            : item.appliedLicenseId !==
                                authorization.data?.latestLicenseId
                              ? "待同步新版本"
                              : item.licenseSyncStatus === "synced"
                                ? "已同步"
                                : "待应用"}
                  </p>
                  {item.licenseSyncError ? (
                    <p className="text-sm text-danger">
                      {item.licenseSyncError}
                    </p>
                  ) : null}
                  {item.containerId &&
                  item.status !== "archived" &&
                  item.status !== "deploying" ? (
                    <Button
                      size="sm"
                      variant="ghost"
                      isDisabled={
                        mutation.pending ||
                        !canDeploy ||
                        item.licenseSyncStatus === "syncing"
                      }
                      onPress={() =>
                        void mutation.run(
                          () =>
                            operationCommand(
                              `/api/platform-instances/${item.instanceId}/sync-license`,
                            ),
                          query.refresh,
                        )
                      }
                    >
                      同步授权
                    </Button>
                  ) : null}
                </div>
                <div className="flex min-w-0 items-start gap-3">
                  <span className="w-20 shrink-0 text-sm leading-6 text-muted">
                    容器名称
                  </span>
                  <div className="min-w-0">
                    <Link
                      href={containerSearchHref(item.containerName)}
                      className="min-w-0 break-all text-sm text-accent underline-offset-4 hover:text-blue-600 hover:underline"
                    >
                      {item.containerName}
                    </Link>
                  </div>
                </div>
                <div className="flex min-w-0 items-start gap-3">
                  <span className="w-20 shrink-0 text-sm leading-6 text-muted">
                    版本包 ID
                  </span>
                  <div className="min-w-0 break-all text-sm text-muted">
                    {item.packageId}
                  </div>
                </div>
                <div className="flex min-w-0 items-start gap-3">
                  <span className="w-20 shrink-0 text-sm leading-6 text-muted">
                    映射端口
                  </span>
                  <div className="min-w-0">
                    Web {item.webPort} / API {item.apiPort}
                  </div>
                </div>
              </div>
            </li>
          ))}
        </ul>
        {!items.length ? (
          <p
            role="status"
            className="rounded-2xl border border-dashed border-default p-8 text-center text-sm text-muted"
          >
            {query.loading
              ? "正在加载实例…"
              : query.error
                ? "实例读取失败，请刷新重试。"
                : search
                  ? "没有匹配的实例。"
                  : "暂无平台实例，Docker 可用时可创建并部署客户平台。"}
          </p>
        ) : null}
      </section>
      {editor && options.data ? (
        <InstanceEditor
          initial={editor.initial}
          options={{
            ...options.data,
            customers: options.data.customers.filter(
              (c) => c.id === customerId,
            ),
          }}
          customerId={customerId}
          onClose={() => setEditor(null)}
          onSaved={() => {
            setEditor(null);
            query.refresh();
          }}
        />
      ) : null}
      {supportInstance ? (
        <InstanceSupportAccess
          item={supportInstance}
          onClose={() => setSupportInstance(null)}
        />
      ) : null}
      {detail ? (
        <ModalShell
          title="平台实例详情"
          width={640}
          onClose={() => setDetail(null)}
        >
          <dl className="space-y-3 break-all text-sm">
            {[
              ["实例名称", detail.name],
              [
                "客户授权",
                detail.authorizationId || "尚未绑定，部署前须补全授权",
              ],
              ["目标许可证", detail.licenseId || "未设置"],
              ["已应用许可证", detail.appliedLicenseId || "未确认"],
              ["授权同步", detail.licenseSyncStatus || "unbound"],
              ["同步说明", detail.licenseSyncError || "无"],
              ["实例标识", detail.instanceId],
              ["客户", detail.customerName],
              ["版本包", `${detail.version} · ${detail.packageId}`],
              ["容器名称", detail.containerName],
              ["容器 ID", detail.containerId || "尚未创建"],
              ["环境变量", detail.environmentKeys?.join("、") || "待配置"],
              ["端口", `Web ${detail.webPort} / API ${detail.apiPort}`],
              ["访问地址", instanceAddress(detail, origin) || "未填写"],
              ["部署结果", instanceStatusLabel[detail.status]],
              ["容器实际状态", instanceRuntimeLabel(detail)],
              ["状态说明", detail.runtime?.message || "无"],
              [
                "最后核对",
                detail.runtime?.checkedAt
                  ? new Date(detail.runtime.checkedAt).toLocaleString("zh-CN")
                  : "等待核对",
              ],
              ["备注", detail.notes || "无"],
            ].map(([name, value]) => (
              <div key={name}>
                <dt className="text-xs text-muted">{name}</dt>
                <dd className="whitespace-pre-wrap">{value}</dd>
              </div>
            ))}
          </dl>
        </ModalShell>
      ) : null}
      {archive ? (
        <ModalShell title="归档平台实例" onClose={() => setArchive(null)}>
          <p className="mb-4 text-sm">
            确定归档“{archive.name}
            ”？归档后不可编辑，保留客户和版本关联记录，释放配置中的端口占用。
          </p>
          {mutation.error ? (
            <p role="alert" className="mb-3 text-sm text-danger">
              {mutation.error}
            </p>
          ) : null}
          <Button isDisabled={mutation.pending} onPress={confirmArchive}>
            确认归档
          </Button>
        </ModalShell>
      ) : null}
      {container ? (
        <ContainerDetails
          key={container.id}
          id={container.id}
          showLogs={container.logs}
          showTerminal={container.terminal}
          initialLogSince={container.since}
          onClose={() => setContainer(null)}
          onChanged={query.refresh}
        />
      ) : null}
      {logId ? (
        <DeploymentLog
          id={logId}
          onClose={() => setLogId(null)}
          containerId={
            query.data?.find((i) => i.instanceId === logId)?.containerId
          }
          onContainerLogs={(id, since) => {
            setLogId(null);
            setContainer({ id, logs: true, since });
          }}
        />
      ) : null}
      {versionInstance && options.data ? (
        <InstanceVersions
          instance={
            query.data?.find(
              (i) => i.instanceId === versionInstance.instanceId,
            ) ?? versionInstance
          }
          options={options.data}
          onClose={() => setVersionInstance(null)}
          onChanged={query.refresh}
        />
      ) : null}
      {settingsOpen ? (
        <DeploymentSettings onClose={() => setSettingsOpen(false)} />
      ) : null}
    </>
  );
}
