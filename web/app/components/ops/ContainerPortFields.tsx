"use client";
import { createClientId } from "../../lib/client-id";
import { Dice6 } from "@gravity-ui/icons";
import { Button, Input } from "@heroui/react";
import ContainerNetworkFields, {
  type ContainerNetwork,
} from "./ContainerNetworkFields";
import type { ContainerPort } from "./container-configuration";
export default function ContainerPortFields({
  network,
  onNetworkChange,
  ports,
  onPortsChange,
  disabled = false,
  allowHost = true,
  allowAddPorts = true,
}: {
  network: ContainerNetwork;
  onNetworkChange: (network: ContainerNetwork) => void;
  ports: ContainerPort[];
  onPortsChange: (ports: ContainerPort[]) => void;
  disabled?: boolean;
  allowHost?: boolean;
  allowAddPorts?: boolean;
}) {
  function randomizePort(id: string) {
    const usedPorts = new Set(ports.map((port) => Number(port.hostPort)));
    const portCount = 65535 - 1024 + 1;
    const start = crypto.getRandomValues(new Uint32Array(1))[0] % portCount;
    for (let offset = 0; offset < portCount; offset++) {
      const candidate = 1024 + ((start + offset) % portCount);
      if (!usedPorts.has(candidate)) {
        onPortsChange(
          ports.map((port) =>
            port.id === id ? { ...port, hostPort: String(candidate) } : port,
          ),
        );
        return;
      }
    }
  }
  return (
    <>
      <ContainerNetworkFields
        value={network}
        onChange={onNetworkChange}
        disabled={disabled}
        allowHost={allowHost}
      />
      {network.networkMode !== "host" ? (
        <div className="space-y-3">
          <p className="text-xs text-muted">
            宿主端口范围
            1024–65535，发布到所有网卡。未添加映射的端口不会对宿主发布。
          </p>
          {ports.map((p, index) => (
            <div
              key={p.id}
              className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-[1fr_1fr_100px_auto]"
            >
              {p.label ? (
                <p className="text-xs text-muted sm:col-span-4">{p.label}</p>
              ) : null}
              <div className="flex min-w-0 items-center gap-2">
                <Button
                  type="button"
                  variant="secondary"
                  isIconOnly
                  isDisabled={disabled}
                  aria-label={`随机${p.label || `宿主端口 ${index + 1}`}`}
                  onPress={() => randomizePort(p.id)}
                >
                  <Dice6 />
                </Button>
                <Input
                  aria-label={p.label || `宿主端口 ${index + 1}`}
                  disabled={disabled}
                  className="min-w-0 flex-1"
                  required
                  type="number"
                  min={1024}
                  max={65535}
                  placeholder="宿主端口"
                  value={p.hostPort}
                  onChange={(e) =>
                    onPortsChange(
                      ports.map((row) =>
                        row.id === p.id
                          ? { ...row, hostPort: e.target.value }
                          : row,
                      ),
                    )
                  }
                />
              </div>
              <Input
                aria-label={`容器端口 ${index + 1}`}
                disabled={disabled || p.managed}
                required
                type="number"
                min={1}
                max={65535}
                placeholder="容器端口"
                value={p.containerPort}
                onChange={(e) =>
                  onPortsChange(
                    ports.map((row) =>
                      row.id === p.id
                        ? { ...row, containerPort: e.target.value }
                        : row,
                    ),
                  )
                }
              />
              <select
                aria-label={`端口协议 ${index + 1}`}
                disabled={disabled || p.managed}
                className="rounded-lg border border-border bg-surface px-2"
                value={p.protocol}
                onChange={(e) =>
                  onPortsChange(
                    ports.map((row) =>
                      row.id === p.id
                        ? { ...row, protocol: e.target.value }
                        : row,
                    ),
                  )
                }
              >
                <option value="tcp">TCP</option>
                <option value="udp">UDP</option>
              </select>
              <Button
                type="button"
                variant="ghost"
                aria-label={`移除端口 ${index + 1}`}
                isDisabled={disabled || p.managed}
                onPress={() =>
                  onPortsChange(ports.filter((row) => row.id !== p.id))
                }
              >
                移除
              </Button>
            </div>
          ))}
          {allowAddPorts ? (
            <Button
              type="button"
              variant="secondary"
              isDisabled={disabled || ports.length >= 32}
              onPress={() =>
                onPortsChange([
                  ...ports,
                  {
                    id: createClientId(),
                    hostPort: "",
                    containerPort: "",
                    protocol: "tcp",
                  },
                ])
              }
            >
              添加端口映射
            </Button>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
