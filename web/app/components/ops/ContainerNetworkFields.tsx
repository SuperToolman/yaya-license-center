"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import { Input, Label, ListBox, Select, Tabs, TextField } from "@heroui/react";
import { useOperationData } from "../../hooks/useOperationData";
import type { InstanceNetworks } from "../../lib/types/platform-delivery";
export type ContainerNetwork = { networkMode: string; networkName: string };
export default function ContainerNetworkFields({
  value,
  onChange,
  allowHost = false,
  disabled = false,
}: {
  value: ContainerNetwork;
  onChange: (value: ContainerNetwork) => void;
  allowHost?: boolean;
  disabled?: boolean;
}) {
  const networks = useOperationData<InstanceNetworks>("/api/platform-networks");
  const matchingNetworks =
    networks.data?.items.filter((n) => n.driver === value.networkMode) ?? [];
  const selectedNetwork = matchingNetworks.find(
    (n) => n.name === value.networkName,
  );
  useErrorNotification(networks.error, networks.loading);
  return (
    <>
      <div className="mb-4">
        <p className="mb-2 text-sm font-medium" id="container-network-mode">
          网络模式
        </p>
        <Tabs
          isDisabled={disabled}
          selectedKey={value.networkMode}
          onSelectionChange={(key) => {
            const mode = String(key);
            if (
              mode !== "bridge" &&
              mode !== "macvlan" &&
              !(allowHost && mode === "host")
            )
              return;
            onChange({
              networkMode: mode,
              networkName:
                networks.data?.items.find((n) => n.driver === mode)?.name ??
                (mode === "host" ? "host" : mode === "bridge" ? "bridge" : ""),
            });
          }}
        >
          <Tabs.ListContainer>
            <Tabs.List aria-labelledby="container-network-mode">
              <Tabs.Tab id="bridge">
                bridge
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="host" isDisabled={!allowHost}>
                host
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="macvlan">
                macvlan
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
        </Tabs>
        <p className="mt-2 text-xs text-muted">
          {allowHost
            ? "host 共享 Docker 宿主网络，不使用端口映射；Docker Desktop 需先启用主机网络支持。"
            : networks.data?.hostReason ||
              "当前离线镜像内置数据库、缓存等固定端口服务，暂不支持 host 共享网络。"}
        </p>
      </div>
      {value.networkMode !== "host" ? (
        <div className="mb-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Select
            isDisabled={disabled}
            isRequired
            fullWidth
            placeholder="请选择网络"
            selectedKey={value.networkName || null}
            onSelectionChange={(key) =>
              onChange({ ...value, networkName: String(key ?? "") })
            }
          >
            <Label>网络名称</Label>
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                {matchingNetworks.length ? (
                  matchingNetworks.map((n) => (
                    <ListBox.Item key={n.name} id={n.name} textValue={n.name}>
                      {n.name}
                      <ListBox.ItemIndicator />
                    </ListBox.Item>
                  ))
                ) : value.networkMode === "bridge" ? (
                  <ListBox.Item id="bridge" textValue="bridge">
                    bridge
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ) : null}
              </ListBox>
            </Select.Popover>
          </Select>
          <TextField fullWidth isReadOnly>
            <Label>子网</Label>
            <Input
              aria-label="网络子网"
              value={selectedNetwork?.subnets.join("、") || ""}
              placeholder={networks.loading ? "正在读取…" : "未配置 / 暂不可用"}
            />
          </TextField>
        </div>
      ) : (
        <p className="mb-4 text-sm text-muted">将直接使用 Docker 宿主网络。</p>
      )}
      {value.networkMode === "macvlan" ? (
        <p role="status" className="mb-3 text-xs text-muted">
          请选择当前 Docker 已有的 macvlan 网络；容器 IP
          由网络分配。宿主互通及外部路由需在 Docker 环境配置。
        </p>
      ) : null}
      {selectedNetwork?.internal ? (
        <p role="status" className="mb-3 text-xs text-warning">
          该网络为内部网络，可能无法访问外部服务，请确认业务需求。
        </p>
      ) : null}
    </>
  );
}
