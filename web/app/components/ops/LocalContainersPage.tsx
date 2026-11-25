"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import ContainerCreateModal from "./ContainerCreateModal";
import ContainerDetails from "./ContainerDetails";
import { ContainerMoreMenu } from "./ContainerMoreMenu";

import { Eye } from "@gravity-ui/icons";
import {
  Button,
  Checkbox,
  Input,
  Label,
  Table,
  Tabs,
  ToggleButton,
  ToggleButtonGroup,
  Tooltip,
  toast,
} from "@heroui/react";
import { useEffect, useRef, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { dockerTabs as tabs, type DockerTab as Tab } from "./docker-routes";
import OperationPageHeader from "../OperationPageHeader";
import { useOperation } from "../OperationProvider";
import { DataPanel } from "../ui/DataPanel";
import { DataTable } from "../ui/DataTable";
import { ModalShell } from "../ui/ModalShell";
import { useOperationResource } from "../../hooks/useOperationQuery";
import {
  operationCommand,
  operationData,
  operationError,
} from "../../lib/operation-api";
import { DockerOverview } from "./DockerOverview";
import { DockerResourceViews } from "./DockerResourceViews";
import { ContainerStateMenu } from "./ContainerStateMenu";
import {
  actionLabels,
  stateLabels,
  portUrl,
  type DockerContainer,
  type DockerContainers,
  type DockerInventory,
  type ContainerAction,
} from "./docker-types";

const percent = (value: number | null) =>
  value == null ? "不可用" : `${value.toFixed(2)}%`;
export default function LocalContainersPage() {
  const { currentUser } = useOperation();
  if (!currentUser) return <p>正在读取账号权限…</p>;
  if (currentUser.role !== "platform_admin")
    return <p role="alert">当前账号没有容器管理访问权限。</p>;
  return <DockerPanel />;
}
function DockerPanel() {
  const [createOpen, setCreateOpen] = useState(false);
  const [detailId, setDetailId] = useState<string | null>(null);
  const pathname = usePathname();
  const router = useRouter();
  const searchParams = useSearchParams();
  const routeTab = pathname.split("/")[3];
  const tab: Tab = tabs.find(([id]) => id === routeTab)?.[0] ?? "overview";
  const query = searchParams.get("search") ?? "";
  const filter = searchParams.get("state") ?? "all";
  function setTab(next: Tab) {
    const suffix = searchParams.toString();
    router.push(`/ops/containers/${next}${suffix ? `?${suffix}` : ""}`, {
      scroll: false,
    });
  }
  function setSearchParam(key: string, value: string) {
    const params = new URLSearchParams(searchParams.toString());
    if (value && !(key === "state" && value === "all")) params.set(key, value);
    else params.delete(key);
    const suffix = params.toString();
    window.history.replaceState(
      null,
      "",
      `${pathname}${suffix ? `?${suffix}` : ""}`,
    );
  }
  const [revision, setRevision] = useState(0);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [confirm, setConfirm] = useState<{
    item: DockerContainer;
    action: ContainerAction;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);
  const containers = useOperationResource<DockerContainers>(
    "/api/ops/local/containers",
    revision,
  );
  const inventory = useOperationResource<DockerInventory>(
    "/api/ops/local/inventory",
    revision,
  );
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (!["overview", "containers"].includes(tab) || !autoRefresh || busy)
      return;
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") setRevision((v) => v + 1);
    }, 5000);
    return () => clearInterval(timer);
  }, [tab, autoRefresh, busy]);
  useErrorNotification(
    containers.error || inventory.error,
    containers.loading || inventory.loading,
  );
  const all = containers.error ? [] : (containers.data?.items ?? []);
  const data = inventory.error ? null : inventory.data;
  function imageName(item: DockerContainer) {
    if (!item.image.startsWith("sha256:")) return item.image;
    const tag = data?.images.find(
      (image) =>
        image.id === item.image &&
        image.repository !== "<none>" &&
        image.tag !== "<none>",
    );
    return tag ? `${tag.repository}:${tag.tag}` : item.image;
  }
  const count = {
    all: containers.data?.total ?? all.length,
    running: all.filter((i) => i.state === "running").length,
    stopped: all.filter((i) => ["exited", "created", "dead"].includes(i.state))
      .length,
    paused: all.filter((i) => i.state === "paused").length,
  };
  const items = all.filter(
    (item) =>
      (filter === "all" ||
        (filter === "running" && item.state === "running") ||
        (filter === "stopped" &&
          ["exited", "created", "dead"].includes(item.state)) ||
        (filter === "paused" && item.state === "paused")) &&
      [item.name, item.image, imageName(item), item.id]
        .join(" ")
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
  );
  const projects = [
    ...new Set(all.map((i) => i.project).filter((v): v is string => !!v)),
  ];
  const refresh = () => setRevision((v) => v + 1);
  const chooseContainers = (value: string) => {
    const params = new URLSearchParams(searchParams.toString());
    if (value === "all") params.delete("state");
    else params.set("state", value);
    const suffix = params.toString();
    router.push(`/ops/containers/containers${suffix ? `?${suffix}` : ""}`, {
      scroll: false,
    });
  };
  async function execute() {
    if (!confirm || busy) return;
    setBusy(true);
    const notification = toast.info("正在提交容器操作…", {
      timeout: 0,
      isLoading: true,
    });
    try {
      const task = await operationCommand<{ taskId: string }>(
        `/api/ops/local/containers/${confirm.item.id}/actions`,
        { action: confirm.action, expectedState: confirm.item.state },
      );
      if (!task) throw Error("操作任务未创建");
      setConfirm(null);
      toast.update(notification, "操作已提交，正在等待 Docker 执行结果…", {
        timeout: 0,
        isLoading: true,
      });
      const deadline = Date.now() + 65000;
      while (Date.now() < deadline) {
        const result = await operationData<{
          status: string;
          error: string | null;
        }>(`/api/ops/local/actions/${task.taskId}`);
        if (result.status !== "running") {
          if (result.status !== "succeeded")
            throw Error(result.error || "操作未完成，请刷新核对实际状态");
          toast.update(notification, "容器操作成功，已刷新实际状态。", {
            variant: "success",
            isLoading: false,
            timeout: 4000,
          });
          if (mounted.current) refresh();
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
      throw Error("等待结果超时，请刷新核对容器实际状态");
    } catch (error) {
      toast.update(notification, operationError(error), {
        variant: "danger",
        isLoading: false,
        timeout: 8000,
      });
      if (mounted.current) refresh();
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="DOCKER"
        title="容器管理"
        description={`管理当前环境的 Docker 容器及资源。当前本机 Docker · ${
          data
            ? `${data.info.operatingSystem} / ${data.info.serverVersion}`
            : "等待连接"
        }`}
        loading={containers.loading || inventory.loading}
        onRefresh={refresh}
        actions={
          <>
            {["overview", "containers"].includes(tab) ? (
              <Checkbox isSelected={autoRefresh} onChange={setAutoRefresh}>
                <Checkbox.Content>
                  <Checkbox.Control>
                    <Checkbox.Indicator />
                  </Checkbox.Control>
                  <Label>每 5 秒自动刷新</Label>
                </Checkbox.Content>
              </Checkbox>
            ) : null}
            {tab === "containers" ? (
              <Button
                isDisabled={!data || !!containers.error}
                onPress={() => setCreateOpen(true)}
              >
                新增容器
              </Button>
            ) : null}
          </>
        }
      />
      {createOpen && data ? (
        <ContainerCreateModal
          inventory={data}
          onClose={() => setCreateOpen(false)}
          onCreated={(id) => {
            setCreateOpen(false);
            refresh();
            setDetailId(id);
          }}
        />
      ) : null}
      <Tabs
        selectedKey={tab}
        onSelectionChange={(key) => {
          if (tabs.some(([id]) => id === key)) setTab(key as Tab);
        }}
      >
        <Tabs.ListContainer className="mb-5 max-w-full overflow-x-auto">
          <Tabs.List aria-label="Docker 资源">
            {tabs.map(([id, label]) => (
              <Tabs.Tab key={id} id={id}>
                {label}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
        {containers.data?.truncated ? (
          <p role="status" className="mb-4 text-sm text-warning">
            仅显示前 {containers.data.limit}{" "}
            个容器，状态分组与编排统计基于当前列表。
          </p>
        ) : null}
        <Tabs.Panel key={tab} id={tab}>
          {tab === "overview" ? (
            <DockerOverview
              data={data}
              count={count}
              ready={!!containers.data && !containers.error}
              projects={projects}
              chooseContainers={chooseContainers}
              setTab={setTab}
            />
          ) : null}
          {tab === "containers" ? (
            <DataPanel
              title="Docker 容器"
              note={`显示 ${items.length} 个容器；点击状态可打开操作菜单。`}
              actions={
                <div className="ml-auto flex flex-wrap items-center justify-end gap-3">
                  <ToggleButtonGroup
                    aria-label="容器状态筛选"
                    selectionMode="single"
                    disallowEmptySelection
                    size="sm"
                    selectedKeys={new Set([filter])}
                    onSelectionChange={(keys) => {
                      const selected = [...keys][0];
                      if (selected != null)
                        setSearchParam("state", String(selected));
                    }}
                  >
                    {(
                      [
                        ["all", "所有"],
                        ["running", "已启动"],
                        ["stopped", "已停止"],
                        ["paused", "已暂停"],
                      ] as const
                    ).map(([value, label]) => (
                      <ToggleButton key={value} id={value}>
                        {label} · {count[value]}
                      </ToggleButton>
                    ))}
                  </ToggleButtonGroup>
                  <Input
                    className="w-60 max-w-full"
                    aria-label="搜索容器"
                    placeholder="搜索名称、镜像或 ID"
                    value={query}
                    onChange={(e) => setSearchParam("search", e.target.value)}
                  />
                </div>
              }
            >
              {containers.data?.statsError ? (
                <p role="status" className="px-4 pb-4 text-sm text-warning">
                  资源使用率暂不可用：{containers.data.statsError}
                </p>
              ) : null}
              <DataTable
                label="Docker 容器"
                columns={[
                  "容器/镜像名称",
                  "状态",
                  "资源使用率",
                  "IP 地址",
                  "端口",
                  "创建时间",
                  "操作",
                ]}
                contentClassName="min-w-[1300px]"
                empty={
                  containers.error
                    ? "容器数据读取失败，请确认 Docker 已启动后刷新。"
                    : containers.loading
                      ? "正在读取容器…"
                      : "没有符合条件的容器"
                }
              >
                {items.map((item) => (
                  <Table.Row key={item.id} id={item.id}>
                    <Table.Cell>
                      <div className="w-64 max-w-full space-y-1">
                        <strong className="block truncate" title={item.name}>
                          {item.name}
                        </strong>
                        <p
                          className="truncate text-xs text-muted"
                          title={item.image}
                        >
                          {imageName(item)}
                        </p>
                      </div>
                    </Table.Cell>
                    <Table.Cell>
                      <ContainerStateMenu
                        item={item}
                        busy={busy}
                        onAction={(action) => {
                          setConfirm({ item, action });
                        }}
                      />
                    </Table.Cell>
                    <Table.Cell>
                      <p className="whitespace-nowrap text-xs">
                        CPU：{percent(item.cpuPercent)}
                      </p>
                      <p
                        className="mt-1 whitespace-nowrap text-xs"
                        title={item.memoryUsage ?? undefined}
                      >
                        内存：{percent(item.memoryPercent)}
                      </p>
                      <p className="mt-1 whitespace-nowrap text-xs text-muted">
                        {item.memoryUsage ?? "—"}
                      </p>
                    </Table.Cell>
                    <Table.Cell>
                      {item.addresses?.length
                        ? item.addresses.map((address) => (
                            <p
                              key={address.network + address.address}
                              className="text-xs"
                              title={address.network}
                            >
                              {address.address}
                            </p>
                          ))
                        : "—"}
                    </Table.Cell>
                    <Table.Cell>
                      <div className="flex max-w-72 flex-col items-start gap-1">
                        {item.ports?.length
                          ? item.ports.map((port, index) => {
                              const href = portUrl(
                                port,
                                typeof window === "undefined"
                                  ? "localhost"
                                  : window.location.hostname,
                              );
                              const label = port.hostPort
                                ? `${port.hostIp || "0.0.0.0"}:${port.hostPort} → ${port.containerPort}`
                                : port.containerPort;
                              return href ? (
                                <a
                                  key={index}
                                  href={href}
                                  target="_blank"
                                  rel="noopener noreferrer"
                                  className="whitespace-nowrap rounded border border-border px-2 py-1 text-xs text-accent hover:underline"
                                  title="尝试通过浏览器打开；非 HTTP 服务可能无法显示网页"
                                >
                                  {label} ↗
                                </a>
                              ) : (
                                <span
                                  key={index}
                                  className="text-xs text-muted"
                                >
                                  {label}
                                </span>
                              );
                            })
                          : "—"}
                      </div>
                    </Table.Cell>
                    <Table.Cell>
                      <span className="whitespace-nowrap text-xs">
                        {item.createdAtMs
                          ? new Date(item.createdAtMs).toLocaleString("zh-CN")
                          : "—"}
                      </span>
                    </Table.Cell>
                    <Table.Cell>
                      <div className="flex items-center gap-1">
                        <Tooltip>
                          <Button
                            isIconOnly
                            size="sm"
                            variant="ghost"
                            aria-label={`详情 ${item.name}`}
                            onPress={() => {
                              setDetailId(item.id);
                            }}
                          >
                            <Eye aria-hidden="true" className="h-4 w-4" />
                          </Button>
                          <Tooltip.Content>查看容器详情</Tooltip.Content>
                        </Tooltip>
                        <ContainerMoreMenu item={item} busy={busy} />
                      </div>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </DataTable>
            </DataPanel>
          ) : null}
          <DockerResourceViews
            tab={tab}
            data={data}
            projects={projects}
            all={all}
          />
        </Tabs.Panel>
      </Tabs>
      {detailId ? (
        <ContainerDetails
          key={detailId}
          id={detailId}
          onClose={() => setDetailId(null)}
          onChanged={refresh}
        />
      ) : null}
      {confirm ? (
        <ModalShell
          title={`${actionLabels[confirm.action]}容器`}
          onClose={() => {
            if (!busy) setConfirm(null);
          }}
        >
          <p className="text-sm">
            确认{actionLabels[confirm.action]}容器{" "}
            <strong>{confirm.item.name}</strong>？当前状态：
            {stateLabels[confirm.item.state] ?? confirm.item.state}。
          </p>
          <p className="mt-3 text-sm text-muted">
            {confirm.action === "kill"
              ? "强制停止会立即终止容器进程，可能中断正在写入的数据。"
              : confirm.action === "pause"
                ? "暂停将冻结容器进程，服务在恢复前无法正常响应。"
                : "此操作可能影响正在访问该容器的用户，请确认后继续。"}
          </p>
          <div className="mt-5 flex justify-end gap-3">
            <Button
              variant="secondary"
              isDisabled={busy}
              onPress={() => setConfirm(null)}
            >
              取消
            </Button>
            <Button isDisabled={busy} onPress={execute}>
              {busy ? "执行中…" : `确认${actionLabels[confirm.action]}`}
            </Button>
          </div>
        </ModalShell>
      ) : null}
    </>
  );
}
