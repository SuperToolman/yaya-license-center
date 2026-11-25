"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import {
  Alert,
  AlertDialog,
  Button,
  Card,
  Chip,
  Separator,
  Spinner,
  Tabs,
  toast,
} from "@heroui/react";
import { useEffect, useRef, useState } from "react";
import { ModalShell } from "../ui/ModalShell";
import {
  operationCommand,
  operationData,
  operationError,
} from "../../lib/operation-api";
import {
  actionLabels,
  stateLabels,
  type ContainerAction,
  type DockerContainer,
} from "./docker-types";

import ContainerTerminal from "./ContainerTerminal";
import ContainerLogs from "./ContainerLogs";
import ContainerMetrics from "./ContainerMetrics";
type Task = {
  taskId: string;
  action: ContainerAction;
  status: string;
  error: string | null;
  createdAtMs: number;
  reconciliation?: { state: string; checkedAt: number } | null;
};
const taskLabels: Record<string, string> = {
  running: "执行中",
  succeeded: "已完成",
  failed: "失败",
  interrupted: "已中断",
};
const panelClassName =
  "min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]";

export default function ContainerDetails({
  id,
  onClose,
  onChanged,
  showLogs = false,
  showTerminal = false,
  initialLogSince,
}: {
  id: string;
  onClose: () => void;
  onChanged?: () => void;
  showLogs?: boolean;
  showTerminal?: boolean;
  initialLogSince?: number;
}) {
  const [item, setItem] = useState<DockerContainer | null>(null);
  const [history, setHistory] = useState<Task[]>([]);
  const [panel, setPanel] = useState(
    showTerminal ? "terminal" : showLogs ? "logs" : "basic",
  );
  const [error, setError] = useState("");

  useErrorNotification(error);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState<ContainerAction | null>(null);
  const live = useRef(true);
  const executing = useRef(false);
  const [pendingAction, setPendingAction] = useState<ContainerAction | null>(
    null,
  );
  const powerAction = item?.state === "running" ? "stop" : "start";
  const load = async () => {
    const results = await Promise.allSettled([
      operationData<DockerContainer>(`/api/ops/local/containers/${id}`),
      operationData<Task[]>(`/api/ops/local/containers/${id}/actions`),
    ]);
    if (!live.current) return;
    if (results[0].status === "fulfilled") {
      setItem(results[0].value);
      setError("");
    } else {
      setItem(null);
      setError(operationError(results[0].reason));
    }
    if (results[1].status === "fulfilled") setHistory(results[1].value);
    else setError(operationError(results[1].reason));
  };
  useEffect(() => {
    live.current = true;
    void Promise.resolve().then(() => {
      if (live.current) {
        void load();
      }
    });
    return () => {
      live.current = false;
    };
    // Identity is stable for the lifetime of this modal; explicit refresh obtains fresh observations.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);
  async function execute() {
    if (!confirm || !item || executing.current) return;
    executing.current = true;
    setPendingAction(confirm);
    setBusy(true);
    setError("");
    const notification = toast.info("容器操作执行中…", {
      timeout: 0,
      isLoading: true,
    });
    try {
      const task = await operationCommand<{ taskId: string }>(
        `/api/ops/local/containers/${id}/actions`,
        { action: confirm, expectedState: item.state },
      );
      if (!task) throw Error("未创建操作任务");
      setConfirm(null);
      const deadline = Date.now() + 65000;
      while (Date.now() < deadline) {
        const result = await operationData<Task>(
          `/api/ops/local/actions/${task.taskId}`,
        );
        if (result.status !== "running") {
          if (result.status !== "succeeded")
            throw Error(result.error || "操作已中断，请核对实际状态");
          toast.update(notification, "操作完成，已核对容器实际状态。", {
            variant: "success",
            isLoading: false,
            timeout: 4000,
          });
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
      throw Error("等待超时，请刷新核对任务记录及实际状态");
    } catch (e) {
      toast.update(notification, operationError(e), {
        variant: "danger",
        isLoading: false,
        timeout: 8000,
      });
    } finally {
      if (live.current) {
        await load();
        if (live.current) {
          setBusy(false);
          setPendingAction(null);
          onChanged?.();
        }
      }
      executing.current = false;
    }
  }
  return (
    <ModalShell
      title={`容器详情 · ${item?.name ?? id.slice(0, 12)}`}
      width={1000}
      height={760}
      bodyClassName="flex min-h-0 flex-1 flex-col overflow-hidden p-5"
      onClose={onClose}
    >
      <div className="flex min-h-0 flex-1 flex-col gap-4">
        <AlertDialog
          isOpen={confirm !== null}
          onOpenChange={(open) => {
            if (!open && !busy) setConfirm(null);
          }}
        >
          <AlertDialog.Backdrop
            isDismissable={false}
            isKeyboardDismissDisabled={busy}
          >
            <AlertDialog.Container placement="center" size="sm">
              <AlertDialog.Dialog>
                <AlertDialog.Header>
                  <AlertDialog.Heading>
                    {confirm ? actionLabels[confirm] : "操作"}容器
                  </AlertDialog.Heading>
                </AlertDialog.Header>
                <AlertDialog.Body>
                  确认{confirm ? actionLabels[confirm] : "操作"}容器“
                  {item?.name}”？
                  {confirm === "start"
                    ? "启动后将恢复容器服务。"
                    : "此操作可能暂时中断客户访问，保留全部数据卷。"}
                </AlertDialog.Body>
                <AlertDialog.Footer>
                  <Button
                    variant="secondary"
                    isDisabled={busy}
                    onPress={() => setConfirm(null)}
                  >
                    取消操作
                  </Button>
                  <Button
                    variant={confirm === "start" ? "primary" : "danger"}
                    isPending={busy}
                    isDisabled={busy}
                    onPress={() => void execute()}
                  >
                    {confirm ? `确认${actionLabels[confirm]}` : "确认"}
                  </Button>
                </AlertDialog.Footer>
              </AlertDialog.Dialog>
            </AlertDialog.Container>
          </AlertDialog.Backdrop>
        </AlertDialog>
        {!item && !error ? <Spinner aria-label="正在读取容器详情" /> : null}
        <Tabs
          className="min-h-0 flex-1"
          selectedKey={panel}
          onSelectionChange={(k) => setPanel(String(k))}
        >
          <Tabs.ListContainer className="shrink-0 overflow-x-auto">
            <Tabs.List aria-label="容器详情分类">
              <Tabs.Tab id="basic">
                基础信息
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="metrics">
                资源监控
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="logs">
                容器日志
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="terminal">
                终端交互
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
          <Tabs.Panel id="basic" className={`${panelClassName} space-y-4`}>
            <Card>
              <Card.Header>
                <Card.Title>基础信息</Card.Title>
              </Card.Header>
              <Card.Content>
                {item ? (
                  <dl className="grid gap-4 sm:grid-cols-2">
                    {[
                      ["容器 ID", item.id],
                      ["镜像", item.image],
                      ["状态", stateLabels[item.state] ?? item.state],
                      ["健康检查", item.health ?? "未配置"],
                      ["退出码", String(item.exitCode ?? "—")],
                      [
                        "创建时间",
                        item.createdAtMs
                          ? new Date(item.createdAtMs).toLocaleString("zh-CN")
                          : "—",
                      ],
                      ["所属客户", item.association?.customerName ?? "未关联"],
                      ["平台实例", item.association?.instanceName ?? "未关联"],
                      [
                        "端口",
                        item.ports
                          .map(
                            (p) =>
                              `${p.hostIp ?? ""}:${p.hostPort ?? "未发布"} → ${p.containerPort}`,
                          )
                          .join("；") || "无",
                      ],
                      [
                        "数据卷",
                        item.mounts
                          ?.map((m) => `${m.name || m.type} → ${m.destination}`)
                          .join("；") || "无",
                      ],
                    ].map(([k, v]) => (
                      <div key={k} className="min-w-0">
                        <dt>
                          <Card.Description>{k}</Card.Description>
                        </dt>
                        <dd className="break-all">{v}</dd>
                      </div>
                    ))}
                  </dl>
                ) : null}
              </Card.Content>
              <Card.Footer className="flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="secondary"
                  isDisabled={busy}
                  onPress={() => void load()}
                >
                  刷新容器详情
                </Button>
                {item ? (
                  <Button
                    size="sm"
                    isDisabled={
                      busy ||
                      item.protected ||
                      !item.allowedActions.includes(powerAction)
                    }
                    isPending={
                      busy &&
                      (pendingAction === "start" || pendingAction === "stop")
                    }
                    onPress={() => setConfirm(powerAction)}
                  >
                    {busy && pendingAction === "start"
                      ? "启动中…"
                      : busy && pendingAction === "stop"
                        ? "停止中…"
                        : `${actionLabels[powerAction]}容器`}
                  </Button>
                ) : null}
                {item?.allowedActions.includes("restart") ? (
                  <Button
                    size="sm"
                    isDisabled={busy || item.protected}
                    isPending={busy && pendingAction === "restart"}
                    onPress={() => setConfirm("restart")}
                  >
                    {busy && pendingAction === "restart"
                      ? "重启中…"
                      : "重启容器"}
                  </Button>
                ) : null}
              </Card.Footer>
            </Card>
            <Card variant="secondary">
              <Card.Header>
                <Card.Title>操作记录 · 最近 30 次</Card.Title>
              </Card.Header>
              <Card.Content className="space-y-3">
                {history.length ? (
                  history.map((t) => (
                    <div key={t.taskId}>
                      <p>
                        {actionLabels[t.action] ?? t.action} ·{" "}
                        <Chip
                          size="sm"
                          variant="soft"
                          color={
                            t.status === "succeeded"
                              ? "success"
                              : t.status === "running"
                                ? "accent"
                                : "danger"
                          }
                        >
                          {taskLabels[t.status] ?? t.status}
                        </Chip>{" "}
                        · {new Date(t.createdAtMs).toLocaleString("zh-CN")}
                      </p>
                      {t.error ? (
                        <p className="text-danger">{t.error}</p>
                      ) : null}
                      {t.reconciliation ? (
                        <p>
                          重启后核对：
                          {stateLabels[t.reconciliation.state] ??
                            t.reconciliation.state}{" "}
                          ·{" "}
                          {new Date(t.reconciliation.checkedAt).toLocaleString(
                            "zh-CN",
                          )}
                        </p>
                      ) : null}
                      <Card.Description>{t.taskId}</Card.Description>
                      <Separator />
                    </div>
                  ))
                ) : (
                  <Card.Description>暂无操作记录</Card.Description>
                )}
              </Card.Content>
            </Card>
          </Tabs.Panel>
          <Tabs.Panel id="metrics" className={panelClassName}>
            <ContainerMetrics id={id} />
          </Tabs.Panel>
          <Tabs.Panel id="logs" className="min-h-0 flex-1 overflow-hidden">
            <ContainerLogs
              id={id}
              initialSince={initialLogSince}
              fillAvailable
            />
          </Tabs.Panel>
          <Tabs.Panel id="terminal" className="min-h-0 flex-1 overflow-hidden">
            {item?.terminalAllowed &&
            !item.protected &&
            item.state === "running" ? (
              <ContainerTerminal id={id} />
            ) : (
              <Alert status="warning">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>终端暂不可用</Alert.Title>
                  <Alert.Description>
                    {item?.protected
                      ? "受保护容器禁止连接终端"
                      : item?.state !== "running"
                        ? "请先启动容器后连接终端"
                        : "当前容器不允许连接终端"}
                  </Alert.Description>
                </Alert.Content>
              </Alert>
            )}
          </Tabs.Panel>
        </Tabs>
      </div>
    </ModalShell>
  );
}
