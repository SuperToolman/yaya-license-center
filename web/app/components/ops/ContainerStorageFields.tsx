"use client";
import { createClientId } from "../../lib/client-id";
import { Button, Input } from "@heroui/react";
import type { ContainerVolume } from "./container-configuration";
export default function ContainerStorageFields({
  volumes,
  onChange,
  disabled = false,
  allowAdd = true,
  description = "使用独立 Docker 命名卷；可填写已有卷或新卷名。yaya- 前缀为客户实例保留。停止、重启不会删除卷。",
}: {
  volumes: ContainerVolume[];
  onChange: (volumes: ContainerVolume[]) => void;
  disabled?: boolean;
  allowAdd?: boolean;
  description?: string;
}) {
  return (
    <div className="space-y-3">
      <p className="text-xs text-muted">{description}</p>
      {volumes.map((v, index) => (
        <div
          key={v.id}
          className="grid min-w-0 grid-cols-1 items-center gap-2 sm:grid-cols-[1fr_1fr_auto_auto]"
        >
          <Input
            aria-label={`数据卷名称 ${index + 1}`}
            required
            disabled={disabled || v.managed}
            placeholder="卷名称"
            value={v.name}
            onChange={(e) =>
              onChange(
                volumes.map((row) =>
                  row.id === v.id ? { ...row, name: e.target.value } : row,
                ),
              )
            }
          />
          <Input
            aria-label={`挂载路径 ${index + 1}`}
            required
            disabled={disabled || v.managed}
            placeholder="容器路径，如 /data"
            value={v.destination}
            onChange={(e) =>
              onChange(
                volumes.map((row) =>
                  row.id === v.id
                    ? { ...row, destination: e.target.value }
                    : row,
                ),
              )
            }
          />
          <label className="flex items-center gap-1 text-sm">
            <input
              type="checkbox"
              disabled={disabled || v.managed}
              checked={v.readOnly}
              onChange={(e) =>
                onChange(
                  volumes.map((row) =>
                    row.id === v.id
                      ? { ...row, readOnly: e.target.checked }
                      : row,
                  ),
                )
              }
            />
            只读
          </label>
          <Button
            type="button"
            variant="ghost"
            aria-label={`移除数据卷 ${index + 1}`}
            isDisabled={disabled || v.managed}
            onPress={() => onChange(volumes.filter((row) => row.id !== v.id))}
          >
            移除
          </Button>
        </div>
      ))}
      {allowAdd ? (
        <Button
          type="button"
          variant="secondary"
          isDisabled={disabled || volumes.length >= 16}
          onPress={() =>
            onChange([
              ...volumes,
              {
                id: createClientId(),
                name: "",
                destination: "",
                readOnly: false,
              },
            ])
          }
        >
          添加数据卷
        </Button>
      ) : null}
    </div>
  );
}
