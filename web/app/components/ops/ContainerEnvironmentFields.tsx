"use client";
import { createClientId } from "../../lib/client-id";

import { CircleMinus, Plus } from "@gravity-ui/icons";
import { Button, Input } from "@heroui/react";
import { type EnvironmentRow } from "./container-configuration";

export default function ContainerEnvironmentFields({
  rows,
  savedKeys = [],
  onChange,
  disabled = false,
  maxNameLength = 128,
  title = "环境变量",
  description = "每行配置一个环境变量；未覆盖的变量沿用镜像默认值。",
}: {
  disabled?: boolean;
  maxNameLength?: number;
  title?: string;
  description?: string;
  rows: EnvironmentRow[];
  savedKeys?: string[];
  onChange: (rows: EnvironmentRow[]) => void;
}) {
  function update(id: string, field: "name" | "value", value: string) {
    onChange(
      rows.map((row) => (row.id === id ? { ...row, [field]: value } : row)),
    );
  }
  return (
    <section aria-label={title} className="min-w-0 space-y-3">
      <h3 className="text-sm font-medium">{title}</h3>
      <p className="text-xs text-muted">{description}</p>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem] items-center gap-2 text-sm">
        <span>变量</span>
        <span>值</span>
        <span className="sr-only">操作</span>
        {rows.map((row, index) => {
          return (
            <div
              key={row.id}
              role="group"
              aria-label={`环境变量 ${index + 1}`}
              className="col-span-3 grid grid-cols-subgrid items-center gap-2"
            >
              <Input
                aria-label={`环境变量名 ${index + 1}`}
                className="min-w-0 font-mono text-xs"
                placeholder="VARIABLE_NAME"
                maxLength={maxNameLength}
                disabled={disabled || row.isReadOnly}
                value={row.name}
                onChange={(event) => update(row.id, "name", event.target.value)}
              />
              <Input
                aria-label={
                  row.name ? `${row.name} 的值` : `环境变量值 ${index + 1}`
                }
                className="min-w-0"
                autoComplete="off"
                disabled={disabled || row.isReadOnly}
                placeholder={
                  savedKeys.includes(row.name)
                    ? [row.description, "已保存，留空保留原值"]
                        .filter(Boolean)
                        .join(" ")
                    : row.description ||
                      (row.isReadOnly ? "由平台管理" : "请输入值")
                }
                value={row.value}
                onChange={(event) =>
                  update(row.id, "value", event.target.value)
                }
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                isIconOnly
                isDisabled={disabled || row.isReadOnly}
                aria-label={`移除环境变量 ${index + 1}`}
                onPress={() =>
                  onChange(rows.filter((item) => item.id !== row.id))
                }
              >
                <CircleMinus />
              </Button>
            </div>
          );
        })}
      </div>
      <Button
        type="button"
        variant="ghost"
        size="sm"
        isDisabled={disabled}
        onPress={() =>
          onChange([...rows, { id: createClientId(), name: "", value: "" }])
        }
      >
        <Plus /> 添加环境变量
      </Button>
    </section>
  );
}
