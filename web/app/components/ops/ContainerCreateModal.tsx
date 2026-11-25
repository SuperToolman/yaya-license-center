"use client";
import { createClientId } from "../../lib/client-id";
import { Input, Label, ListBox, Select, TextField } from "@heroui/react";
import { useState } from "react";
import { ModalShell } from "../ui/ModalShell";
import ContainerCreateForm from "./ContainerCreateForm";
import {
  serializeContainerEnvironment,
  type ContainerPort,
  type ContainerVolume,
  type EnvironmentRow,
} from "./container-configuration";
import type { DockerInventory } from "./docker-types";
import {
  operationCommand,
  operationData,
  operationError,
} from "../../lib/operation-api";

type Task = {
  creationId: string;
  containerId?: string | null;
  status: string;
  error?: string | null;
};
export default function ContainerCreateModal({
  inventory,
  onClose,
  onCreated,
}: {
  inventory: DockerInventory;
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const [creationId] = useState(() => createClientId());
  const [name, setName] = useState("");
  const [imageId, setImageId] = useState("");
  const [network, setNetwork] = useState({
    networkMode: "bridge",
    networkName: "bridge",
  });
  const [ports, setPorts] = useState<ContainerPort[]>([]);
  const [volumes, setVolumes] = useState<ContainerVolume[]>([]);
  const [environment, setEnvironment] = useState<EnvironmentRow[]>([]);
  const [restartPolicy, setRestart] = useState("unless-stopped");
  const [start, setStart] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const [submitted, setSubmitted] = useState(false);
  const [finished, setFinished] = useState(false);
  const [containerId, setContainerId] = useState<string | null>(null);
  const images = [...new Map(inventory.images.map((i) => [i.id, i])).values()];
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (pending || finished) return;
    setPending(true);
    setError("");
    try {
      let task: Task | null;
      if (submitted) {
        try {
          task = await operationData<Task>(
            `/api/ops/local/container-creations/${creationId}`,
          );
        } catch (e) {
          setSubmitted(false);
          throw e;
        }
      } else {
        const env = serializeContainerEnvironment(environment);
        // Keep the same key after network errors so retry can only observe the original operation.
        setSubmitted(true);
        task = await operationCommand<Task>("/api/ops/local/containers", {
          creationId,
          name,
          imageId,
          ...network,
          ports:
            network.networkMode === "host"
              ? []
              : ports.map((p) => ({
                  hostPort: Number(p.hostPort),
                  containerPort: Number(p.containerPort),
                  protocol: p.protocol,
                })),
          volumes: volumes.map(({ name, destination, readOnly }) => ({
            name,
            destination,
            readOnly,
          })),
          environment: env,
          restartPolicy,
          start,
        });
      }
      const until = Date.now() + 65000;
      while (task?.status === "running" && Date.now() < until) {
        await new Promise((r) => setTimeout(r, 800));
        task = await operationData<Task>(
          `/api/ops/local/container-creations/${creationId}`,
        );
      }
      if (task?.status === "succeeded" && task.containerId) {
        onCreated(task.containerId);
        return;
      }
      if (task?.containerId) setContainerId(task.containerId);
      if (task && task.status !== "running") setFinished(true);
      throw Error(
        task?.error || "创建任务仍在执行，请点击核对创建结果，避免重复创建。",
      );
    } catch (e) {
      setError(operationError(e));
    } finally {
      setPending(false);
    }
  }
  return (
    <ModalShell
      title="新增容器"
      width={1000}
      onClose={() => {
        if (!pending) onClose();
      }}
    >
      <ContainerCreateForm
        onSubmit={submit}
        onClose={onClose}
        pending={pending}
        disabled={!imageId || !network.networkName || finished}
        error={error}
        description="使用当前 Docker 已有镜像创建通用容器。客户低代码平台请从客户管理中部署。"
        submitLabel={
          submitted ? "核对创建结果" : start ? "创建并启动" : "创建容器"
        }
        basic={
          <div className="grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-2">
            <Select
              isRequired
              fullWidth
              selectedKey={imageId || null}
              onSelectionChange={(k) => setImageId(String(k ?? ""))}
              isDisabled={submitted}
            >
              <Label>镜像</Label>
              <Select.Trigger>
                <Select.Value />
                <Select.Indicator />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  {images.map((i) => (
                    <ListBox.Item
                      key={i.id}
                      id={i.id}
                      textValue={
                        i.repository !== "<none>"
                          ? `${i.repository}:${i.tag}`
                          : i.id
                      }
                    >
                      {i.repository !== "<none>"
                        ? `${i.repository}:${i.tag}`
                        : i.id}
                      <ListBox.ItemIndicator />
                    </ListBox.Item>
                  ))}
                </ListBox>
              </Select.Popover>
            </Select>
            <TextField isRequired fullWidth isDisabled={submitted}>
              <Label>容器名称</Label>
              <Input
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={128}
                pattern="[a-zA-Z0-9][a-zA-Z0-9_.-]*"
                placeholder="例如 nginx-demo"
              />
            </TextField>
            <p className="text-xs text-muted sm:col-span-2">
              仅使用本地镜像，不自动拉取或上传镜像。未找到镜像时请先导入或在本机拉取，再刷新。
            </p>
          </div>
        }
        configuration={{
          storage: {
            volumes,
            onChange: setVolumes,
            disabled: pending || submitted,
          },
          network: {
            network,
            onNetworkChange: setNetwork,
            ports,
            onPortsChange: setPorts,
            disabled: pending || submitted,
          },
          environment: {
            rows: environment,
            onChange: setEnvironment,
            disabled: pending || submitted,
          },
        }}
        other={
          <div className="space-y-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <label className="flex flex-col gap-2 text-sm">
                重启策略
                <select
                  aria-label="重启策略"
                  disabled={pending || submitted}
                  className="rounded-lg border border-border bg-surface p-2"
                  value={restartPolicy}
                  onChange={(e) => setRestart(e.target.value)}
                >
                  <option value="no">不自动重启</option>
                  <option value="unless-stopped">除非手动停止</option>
                  <option value="always">始终重启</option>
                  <option value="on-failure">异常退出时重启</option>
                </select>
              </label>
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  disabled={pending || submitted}
                  checked={start}
                  onChange={(e) => setStart(e.target.checked)}
                />
                创建后立即启动
              </label>
            </div>
            {submitted ? (
              <p role="status" className="break-all text-xs text-muted">
                创建任务：{creationId}。重试仅核对该任务，不会重复创建。
                {finished
                  ? "请关闭后在容器列表核对实际状态；如需修改配置，请重新打开表单。"
                  : ""}
              </p>
            ) : null}
            {containerId ? (
              <p className="break-all text-xs text-muted">
                容器 ID：{containerId}
              </p>
            ) : null}
          </div>
        }
      />
    </ModalShell>
  );
}
