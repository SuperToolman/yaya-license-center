"use client";
import {
  Description,
  Input,
  Label,
  ListBox,
  Select,
  TextArea,
  TextField,
} from "@heroui/react";
import { useState } from "react";
import {
  initialEnvironmentRows,
  serializeEnvironment,
} from "../lib/environment";
import { ModalShell } from "../../../../components/ui/ModalShell";
import { useCustomerAuthorization } from "../../../../components/CustomerAuthorizationPanel";
import ContainerCreateForm from "../../../../components/ops/ContainerCreateForm";
import { useOperationMutation } from "../../../../hooks/useOperationMutation";
import { operationCommand } from "../../../../lib/operation-api";
import type {
  InstanceOptions,
  PlatformInstance,
} from "../../../../lib/types/platform-delivery";

export default function InstanceEditor({
  initial,
  customerId,
  options,
  onClose,
  onSaved,
}: {
  initial: PlatformInstance | null;
  customerId: string;
  options: InstanceOptions;
  onClose: () => void;
  onSaved: () => void;
}) {
  const authorization = useCustomerAuthorization(customerId);
  const license = authorization.data?.latest;
  const canDeploy =
    !!license &&
    license.platformStatus !== "destroyed" &&
    license.platformStatus !== "expired";
  const [form, setForm] = useState({
    networkMode: initial?.networkMode ?? "bridge",
    networkName: initial?.networkName ?? "bridge",
    customerId: customerId,
    packageId: initial?.packageId ?? "",
    webPort: String(initial?.webPort ?? 8787),
    apiPort: String(initial?.apiPort ?? 8788),
    publicUrl: initial?.publicUrl ?? "",
    notes: initial?.notes ?? "",
  });
  const [environmentRows, setEnvironmentRows] = useState(() =>
    initialEnvironmentRows(
      options.packages.find((item) => item.id === initial?.packageId)
        ?.environment ?? [],
      initial?.environmentKeys,
      options.packages.find((item) => item.id === initial?.packageId)
        ?.deployment?.environment,
    ),
  );
  const selectedPackage = options.packages.find(
    (item) => item.id === form.packageId,
  );
  const instanceName =
    initial?.name ??
    (selectedPackage && form.customerId
      ? `lowcode-${selectedPackage.name}-${form.customerId}`
      : "");
  function selectPackage(packageId: string) {
    if (packageId === form.packageId) return;
    field("packageId", packageId);
    setEnvironmentRows(
      initialEnvironmentRows(
        options.packages.find((item) => item.id === packageId)?.environment ??
          [],
        initial?.environmentKeys,
        options.packages.find((item) => item.id === packageId)?.deployment
          ?.environment,
      ),
    );
  }
  const mutation = useOperationMutation();
  const [savedId, setSavedId] = useState<string | null>(null);
  const field = (key: keyof typeof form, value: string) =>
    setForm((old) => ({ ...old, [key]: value }));
  async function submit(event: React.FormEvent) {
    event.preventDefault();
    await mutation.run(async () => {
      const result = await operationCommand<PlatformInstance>(
        initial || savedId
          ? `/api/platform-instances/${initial?.instanceId ?? savedId}`
          : "/api/platform-instances",
        {
          ...form,
          licenseId: license?.licenseId,
          name: instanceName,
          environment: serializeEnvironment(environmentRows),
          webPort: Number(form.webPort),
          apiPort: Number(form.apiPort),
        },
        initial || savedId ? "PUT" : "POST",
      );
      if (!initial && result) {
        setSavedId(result.instanceId);
        await operationCommand(
          `/api/platform-instances/${result.instanceId}/deploy`,
        );
      }
      return result;
    }, onSaved);
  }
  return (
    <ModalShell
      title={initial ? "编辑平台实例" : "新建平台实例"}
      width={1000}
      onClose={onClose}
    >
      <ContainerCreateForm
        onSubmit={submit}
        description="确认版本及网络配置后部署到当前环境的 Docker，平台自动配置数据库、密钥及客户许可证。"
        error={mutation.error}
        pending={mutation.pending}
        disabled={!form.networkName || !canDeploy || !!authorization.error}
        onClose={onClose}
        submitLabel={initial ? "保存配置" : "创建并部署"}
        basic={
          <>
            <p className="mb-4 break-all text-sm">
              客户授权：
              {authorization.loading
                ? "正在读取…"
                : authorization.error ||
                  (!canDeploy
                    ? "暂无有效平台授权，请先完成订单授权交付"
                    : `${authorization.data?.authorizationId} · 第 ${authorization.data?.revision} 版`)}
            </p>
            <div className="grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-2">
              <Select
                isRequired
                fullWidth
                placeholder="请先选择版本包"
                selectedKey={form.packageId || null}
                onSelectionChange={(key) => selectPackage(String(key ?? ""))}
              >
                <Label>平台版本包</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {options.packages.map((item) => (
                      <ListBox.Item
                        key={item.id}
                        id={item.id}
                        textValue={`${item.name} · ${item.id}`}
                      >
                        {item.name} · {item.id}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
              <Select
                isRequired
                fullWidth
                placeholder="请选择客户"
                isDisabled
                selectedKey={form.customerId || null}
                onSelectionChange={(key) =>
                  field("customerId", String(key ?? ""))
                }
              >
                <Label>所属客户</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {options.customers.map((item) => (
                      <ListBox.Item
                        key={item.id}
                        id={item.id}
                        textValue={item.name}
                      >
                        {item.name}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
            </div>
            <div className="mt-4 grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-2">
              <TextField isDisabled fullWidth>
                <Label>实例名称</Label>
                <Input
                  aria-label="实例名称"
                  placeholder="选择平台版本包和所属客户后自动生成"
                  value={instanceName}
                />
              </TextField>
              <TextField isDisabled fullWidth>
                <Label>容器名称</Label>
                <Input
                  aria-label="容器名称"
                  placeholder="选择平台版本包和所属客户后自动分配"
                  value={
                    initial?.containerName ??
                    (instanceName ? `${instanceName}-序号` : "")
                  }
                />
              </TextField>
            </div>
          </>
        }
        configuration={{
          storage: {
            volumes: platformVolumes(selectedPackage, initial?.instanceId),
            onChange: () => {},
            allowAdd: false,
            disabled: mutation.pending,
            description:
              "平台必需数据卷由镜像部署清单指定并自动创建，停止、重启保留数据。卷名在创建实例后分配。",
          },
          network: {
            network: form,
            onNetworkChange: (network) =>
              setForm((old) => ({
                ...old,
                ...network,
                networkMode: network.networkMode as "bridge" | "macvlan",
              })),
            ports: [
              {
                id: "web",
                label: "Web 端口",
                hostPort: form.webPort,
                containerPort: String(
                  selectedPackage?.deployment?.webPort ?? 8787,
                ),
                protocol: "tcp",
                managed: true,
              },
              {
                id: "api",
                label: "API 端口",
                hostPort: form.apiPort,
                containerPort: String(
                  selectedPackage?.deployment?.apiPort ?? 8788,
                ),
                protocol: "tcp",
                managed: true,
              },
            ],
            onPortsChange: (ports) =>
              setForm((old) => ({
                ...old,
                webPort:
                  ports.find((p) => p.id === "web")?.hostPort ?? old.webPort,
                apiPort:
                  ports.find((p) => p.id === "api")?.hostPort ?? old.apiPort,
              })),
            allowHost: false,
            allowAddPorts: false,
            disabled: mutation.pending,
          },
          environment: form.packageId
            ? {
                rows: environmentRows,
                savedKeys: initial?.environmentKeys,
                onChange: setEnvironmentRows,
                disabled: mutation.pending,
                maxNameLength: 80,
                title: "平台环境变量",
                description:
                  "以下配置来自版本包部署清单及镜像。数据库、缓存、密钥、端口、路径与内部连接由平台自动配置，无需填写；每个实例使用独立密钥，编辑时保留原值。其他可选配置可按需覆盖。客户许可证与验签公钥将在部署时自动写入并校验，校验成功后才完成部署。",
              }
            : undefined,
        }}
        networkExtra={
          <TextField fullWidth>
            <Label>平台域名</Label>
            <Input
              aria-label="平台域名"
              placeholder="选填，例如 https://customer.example.com"
              maxLength={500}
              value={form.publicUrl}
              onChange={(e) => field("publicUrl", e.target.value)}
            />
            <Description>
              可留空，通过服务器 IP 和 Web 映射端口访问。填写域名时需另行配置
              DNS、网关转发与 HTTPS 证书。
            </Description>
          </TextField>
        }
        environmentNotice={
          !form.packageId ? (
            <p className="text-sm text-muted">
              请先选择平台版本包，再配置环境变量。
            </p>
          ) : selectedPackage?.environment == null &&
            !selectedPackage?.deployment?.environment?.length ? (
            <p role="status" className="text-sm text-muted">
              此版本包未携带镜像环境变量快照，请使用新版打包功能重新打包并上传；当前可手动添加变量。
            </p>
          ) : null
        }
        other={
          <>
            <div className="flex flex-col gap-4">
              <TextField fullWidth>
                <Label>备注</Label>
                <TextArea
                  aria-label="备注"
                  maxLength={2000}
                  rows={3}
                  value={form.notes}
                  onChange={(e) => field("notes", e.target.value)}
                />
              </TextField>
            </div>
          </>
        }
      />
    </ModalShell>
  );
}

function platformVolumes(
  selectedPackage: InstanceOptions["packages"][number] | undefined,
  instanceId?: string,
) {
  if (!selectedPackage) return [];
  const volumes = selectedPackage.deployment?.volumes ?? [
    { key: "database", destination: "/var/lib/postgresql" },
    { key: "platform-state", destination: "/var/lib/yaya" },
  ];
  return volumes.map((volume) => ({
    id: volume.key,
    name: `yaya-${instanceId ?? "实例标识"}-${volume.key === "database" ? "postgres" : volume.key === "platform-state" ? "state" : volume.key}`,
    destination: volume.destination,
    readOnly: false,
    managed: true,
  }));
}
