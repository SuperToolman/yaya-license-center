"use client";
import { createClientId } from "../../../../lib/client-id";
import {
  Button,
  Label,
  ListBox,
  Select,
  Table,
  Tabs,
  TextArea,
} from "@heroui/react";
import { useEffect, useState } from "react";
import { ModalShell } from "../../../../components/ui/ModalShell";
import { DataTable } from "../../../../components/ui/DataTable";
import { useOperationData } from "../../../../hooks/useOperationData";
import { useOperationMutation } from "../../../../hooks/useOperationMutation";
import { operationUrl, operationCommand } from "../../../../lib/operation-api";
import type {
  PlatformInstance,
  InstanceOptions,
} from "../../../../lib/types/platform-delivery";
type Point = {
  id: string;
  revisionId: string;
  status: string;
  size: number;
  createdAt: number;
  error?: string;
};
type Revision = {
  id: string;
  packageId: string;
  containerId: string;
  imageId: string;
  createdAt: number;
};
type Task = {
  id: string;
  kind: string;
  stage: string;
  status: string;
  error?: string;
  recoveryError?: string;
};
type History = {
  recoveryPoints: Point[];
  revisions: Revision[];
  tasks: Task[];
};
type Preview = {
  environment: {
    name: string;
    current: boolean;
    target: boolean;
    required: boolean;
    default?: string;
    action: string;
  }[];
  volumes: {
    key: string;
    destination: string;
    sourceDestination?: string;
    action: string;
  }[];
  targetImageId: string;
  message: string;
};
const labels: Record<string, string> = {
  queued: "排队中",
  running: "执行中",
  recovering: "正在恢复原修订",
  attention: "中断，待核对",
  succeeded: "成功",
  failed: "失败",
  deleted: "已删除",
  interrupted: "中断，已核对",
  ready: "可恢复",
  creating: "创建中",
  backup: "备份",
  upgrade: "升级",
  restore: "回退恢复",
  preparing: "准备镜像与配置",
  stopping: "停止原容器",
  backing_up: "加密备份与校验",
  restoring_volumes: "创建并恢复新卷",
  checking: "健康检查",
  switching: "切换实例绑定",
  completed: "完成",
  reconciled: "已核对",
  added: "新增",
  removed: "移除",
  inherited: "继承",
  clone: "从备份恢复至新卷",
  create: "创建空卷",
};
export default function InstanceVersions({
  instance,
  options,
  onClose,
  onChanged,
}: {
  instance: PlatformInstance;
  options: InstanceOptions;
  onClose: () => void;
  onChanged: () => void;
}) {
  const base = `/api/platform-instances/${encodeURIComponent(instance.instanceId)}`;
  const history = useOperationData<History>(`${base}/versions`);
  const mutation = useOperationMutation();
  const [tab, setTab] = useState("upgrade");
  const [packageId, setPackage] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [overrides, setOverrides] = useState("");
  const [removed, setRemoved] = useState("");
  const [selection, setSelection] = useState<Point | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [notice, setNotice] = useState("");
  const [deletePoint, setDeletePoint] = useState<string | null>(null);
  const busy = (history.data?.tasks ?? []).some((t) =>
    ["queued", "running", "recovering", "attention"].includes(t.status),
  );
  const refresh = history.refresh;
  useEffect(() => {
    const timer = setInterval(() => {
      if (!document.hidden) {
        refresh();
        onChanged();
      }
    }, 3000);
    return () => clearInterval(timer);
  }, [refresh, onChanged]);
  function request(kind: string) {
    const environment: Record<string, string> = {};
    for (const line of overrides.split("\n").filter((x) => x.trim())) {
      const i = line.indexOf("=");
      if (i < 1) throw Error("环境变量应按每行 KEY=value 填写");
      environment[line.slice(0, i).trim()] = line.slice(i + 1);
    }
    return {
      taskId: createClientId(),
      kind,
      expectedContainerId: instance.containerId,
      packageId: kind === "upgrade" ? packageId : undefined,
      recoveryPointId: kind === "restore" ? selection?.id : undefined,
      environment,
      removeEnvironment: removed
        .split(/[\n,]/)
        .map((s) => s.trim())
        .filter(Boolean),
      acknowledgeDataRestore: confirmed,
    };
  }
  async function inspect() {
    await mutation.run(async () => {
      const result = await operationCommand<Preview>(
        `${base}/version-preview`,
        request("upgrade"),
      );
      setPreview(result);
    });
  }
  async function downloadKey() {
    await mutation.run(async () => {
      const response = await fetch(
        operationUrl("/api/platform-recovery/key-export"),
        { method: "POST", credentials: "include" },
      );
      if (!response.ok) throw Error("备份密钥导出失败");
      const url = URL.createObjectURL(await response.blob());
      const a = document.createElement("a");
      a.href = url;
      a.download = "yaya-recovery-master.key";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  }
  async function execute(kind: string) {
    await mutation.run(async () => {
      await operationCommand(`${base}/version-tasks`, request(kind));
      setNotice(
        "已提交任务。窗口可关闭，任务会继续执行；失败时自动恢复原修订。",
      );
      setSelection(null);
      setConfirmed(false);
      refresh();
      onChanged();
    });
  }
  return (
    <ModalShell
      title={`${instance.name} · 版本与恢复点`}
      width={1100}
      onClose={onClose}
    >
      <p className="mb-4 text-sm text-muted">
        升级前停机并创建加密恢复点，新版本使用独立数据卷。原容器、原卷与旧镜像保留；备份和恢复期间不可启停或连接终端。
      </p>
      {mutation.error || history.error ? (
        <p role="alert" className="mb-3 text-danger">
          {mutation.error || history.error}
        </p>
      ) : null}
      {notice ? (
        <p role="status" className="mb-3 text-success">
          {notice}
        </p>
      ) : null}
      {history.data?.tasks.some((t) => t.status === "attention") ? (
        <Button
          variant="danger"
          isDisabled={mutation.pending}
          onPress={() =>
            void mutation.run(async () => {
              await operationCommand(`${base}/version-reconcile`);
              refresh();
              onChanged();
            })
          }
        >
          核对并恢复原修订
        </Button>
      ) : null}
      <Tabs selectedKey={tab} onSelectionChange={(k) => setTab(String(k))}>
        <Tabs.ListContainer>
          <Tabs.List aria-label="版本管理">
            {[
              ["upgrade", "版本升级"],
              ["points", "恢复点"],
              ["history", "部署修订"],
              ["tasks", "任务记录"],
            ].map(([id, name]) => (
              <Tabs.Tab id={id} key={id}>
                {name}
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
        <Tabs.Panel id={tab} className="space-y-4 pt-4">
          {tab === "upgrade" ? (
            <>
              <Select
                aria-label="目标平台版本"
                selectedKey={packageId}
                onSelectionChange={(k) => {
                  setPackage(String(k));
                  setPreview(null);
                }}
              >
                <Select.Trigger>
                  <Select.Value />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {options.packages
                      .filter((p) => p.id !== instance.packageId)
                      .map((p) => (
                        <ListBox.Item id={p.id} key={p.id}>
                          {p.name} · {p.id}
                        </ListBox.Item>
                      ))}
                  </ListBox>
                </Select.Popover>
              </Select>
              <Button
                variant="secondary"
                isDisabled={!packageId || mutation.pending || busy}
                onPress={() => void inspect()}
              >
                预览版本变更
              </Button>
              {preview ? (
                <>
                  <p className="break-all text-sm">
                    目标镜像：{preview.targetImageId}
                  </p>
                  <DataTable
                    label="环境变量差异"
                    columns={["变量", "变化", "必填", "目标默认值"]}
                    empty="无变量"
                  >
                    <>
                      {preview.environment.map((e) => (
                        <Table.Row key={e.name}>
                          <Table.Cell>{e.name}</Table.Cell>
                          <Table.Cell>{labels[e.action]}</Table.Cell>
                          <Table.Cell>{e.required ? "是" : "否"}</Table.Cell>
                          <Table.Cell>
                            {e.default ?? "保留已有值 / 无默认值"}
                          </Table.Cell>
                        </Table.Row>
                      ))}
                    </>
                  </DataTable>
                  <Label>环境变量覆盖（仅填写本次修改，敏感值不回显）</Label>
                  <TextArea
                    aria-label="升级环境变量"
                    value={overrides}
                    onChange={(e) => setOverrides(e.target.value)}
                    placeholder="KEY=value"
                  />
                  <Label>明确移除的变量（每行一个名称）</Label>
                  <TextArea
                    aria-label="移除环境变量"
                    value={removed}
                    onChange={(e) => setRemoved(e.target.value)}
                  />
                  <DataTable
                    label="存储空间差异"
                    columns={["用途", "目标路径", "来源路径", "处理"]}
                    empty="无持久卷"
                  >
                    <>
                      {preview.volumes.map((v) => (
                        <Table.Row key={v.key}>
                          <Table.Cell>{v.key}</Table.Cell>
                          <Table.Cell>{v.destination}</Table.Cell>
                          <Table.Cell>{v.sourceDestination ?? "—"}</Table.Cell>
                          <Table.Cell>{labels[v.action]}</Table.Cell>
                        </Table.Row>
                      ))}
                    </>
                  </DataTable>
                  <p className="text-sm text-muted">
                    不会将旧版移除的卷挂入新版，也不会删除这些卷。当前卷仅支持独占的
                    Docker 命名卷。升级期间停止服务，备份空间不足时终止。
                  </p>
                  <label className="flex items-start gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={confirmed}
                      onChange={(e) => setConfirmed(e.target.checked)}
                    />
                    确认停机、创建恢复点，并使用目标配置与独立数据卷升级
                  </label>
                  <Button
                    isDisabled={!confirmed || busy || mutation.pending}
                    onPress={() => void execute("upgrade")}
                  >
                    备份并升级
                  </Button>
                </>
              ) : null}
            </>
          ) : null}
          {tab === "points" ? (
            <>
              <div className="flex gap-2">
                <Button
                  isDisabled={busy || mutation.pending}
                  onPress={() => void execute("backup")}
                >
                  创建恢复点
                </Button>
                <Button variant="secondary" onPress={() => void downloadKey()}>
                  导出备份密钥
                </Button>
              </div>
              <p className="text-sm text-muted">
                恢复点包含加密配置快照与数据卷归档；恢复会先备份当前版本，再恢复到独立新卷。备份密钥必须单独妥善保存。
              </p>
              <DataTable
                label="实例恢复点"
                columns={["恢复点", "创建时间", "大小", "状态", "操作"]}
                empty="暂无恢复点"
              >
                {(history.data?.recoveryPoints ?? []).map((p) => (
                  <Table.Row key={p.id}>
                    <Table.Cell>
                      <span className="break-all text-xs">{p.id}</span>
                      {p.error ? (
                        <p className="text-danger">{p.error}</p>
                      ) : null}
                    </Table.Cell>
                    <Table.Cell>
                      {new Date(p.createdAt).toLocaleString()}
                    </Table.Cell>
                    <Table.Cell>
                      {(p.size / 1024 / 1024).toFixed(2)} MiB
                    </Table.Cell>
                    <Table.Cell>{labels[p.status] ?? p.status}</Table.Cell>
                    <Table.Cell>
                      <Button
                        size="sm"
                        variant="secondary"
                        isDisabled={p.status !== "ready" || busy}
                        onPress={() => {
                          setSelection(p);
                          setConfirmed(false);
                        }}
                      >
                        恢复到此版本
                      </Button>
                      {p.status === "ready" ? (
                        <a
                          className="mx-2 text-accent"
                          href={operationUrl(
                            `${base}/recovery-points/${p.id}/download`,
                          )}
                        >
                          下载
                        </a>
                      ) : null}
                      <Button
                        size="sm"
                        variant="ghost"
                        isDisabled={
                          p.status !== "ready" || busy || mutation.pending
                        }
                        onPress={() =>
                          void mutation.run(async () => {
                            await operationCommand(
                              `${base}/recovery-points/${p.id}/verify`,
                            );
                            setNotice("恢复点校验通过");
                          })
                        }
                      >
                        校验
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        isDisabled={
                          busy || mutation.pending || p.status === "deleted"
                        }
                        onPress={() => setDeletePoint(p.id)}
                      >
                        删除
                      </Button>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </DataTable>
              {deletePoint ? (
                <div className="space-y-3 rounded-lg border border-danger p-4">
                  <p>
                    确认删除此恢复点？最近可用恢复点受保护。删除后无法恢复对应归档。
                  </p>
                  <Button
                    variant="danger"
                    isDisabled={busy || mutation.pending}
                    onPress={() =>
                      void mutation.run(async () => {
                        await operationCommand(
                          `${base}/recovery-points/${deletePoint}`,
                          undefined,
                          "DELETE",
                        );
                        setDeletePoint(null);
                        refresh();
                      })
                    }
                  >
                    确认删除
                  </Button>
                  <Button
                    variant="secondary"
                    onPress={() => setDeletePoint(null)}
                  >
                    取消
                  </Button>
                </div>
              ) : null}
              {selection ? (
                <div className="space-y-3 rounded-lg border border-danger p-4">
                  <p>
                    将恢复至 {new Date(selection.createdAt).toLocaleString()}
                    。该时刻之后的数据不会出现在恢复后的系统中，当前数据会先生成新的恢复点。
                  </p>
                  <label className="flex gap-2">
                    <input
                      type="checkbox"
                      checked={confirmed}
                      onChange={(e) => setConfirmed(e.target.checked)}
                    />
                    确认恢复历史程序、配置和数据
                  </label>
                  <Button
                    variant="danger"
                    isDisabled={!confirmed || mutation.pending || busy}
                    onPress={() => void execute("restore")}
                  >
                    备份当前版本并恢复
                  </Button>
                  <Button variant="ghost" onPress={() => setSelection(null)}>
                    取消
                  </Button>
                </div>
              ) : null}
            </>
          ) : null}
          {tab === "history" ? (
            <DataTable
              label="部署修订"
              columns={["时间", "版本包", "镜像", "容器"]}
              empty="首次创建恢复点后生成修订"
            >
              {(history.data?.revisions ?? []).map((r) => (
                <Table.Row key={r.id}>
                  <Table.Cell>
                    {new Date(r.createdAt).toLocaleString()}
                  </Table.Cell>
                  <Table.Cell>
                    <span className="break-all text-xs">{r.packageId}</span>
                  </Table.Cell>
                  <Table.Cell>
                    <span className="break-all text-xs">{r.imageId}</span>
                  </Table.Cell>
                  <Table.Cell>
                    <span className="break-all text-xs">{r.containerId}</span>
                  </Table.Cell>
                </Table.Row>
              ))}
            </DataTable>
          ) : null}
          {tab === "tasks" ? (
            <DataTable
              label="版本任务"
              columns={["任务", "状态", "阶段", "诊断"]}
              empty="暂无版本任务"
            >
              {(history.data?.tasks ?? []).map((t) => (
                <Table.Row key={t.id}>
                  <Table.Cell>
                    {labels[t.kind] ?? t.kind}
                    <p className="text-xs text-muted">{t.id}</p>
                  </Table.Cell>
                  <Table.Cell>{labels[t.status] ?? t.status}</Table.Cell>
                  <Table.Cell>{labels[t.stage] ?? t.stage}</Table.Cell>
                  <Table.Cell>
                    <p className="text-danger">{t.error}</p>
                    {t.recoveryError ? (
                      <p className="text-danger">
                        原版本恢复失败：{t.recoveryError}
                      </p>
                    ) : null}
                  </Table.Cell>
                </Table.Row>
              ))}
            </DataTable>
          ) : null}
        </Tabs.Panel>
      </Tabs>
    </ModalShell>
  );
}
