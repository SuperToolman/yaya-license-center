"use client";

import { Ellipsis } from "@gravity-ui/icons";
import { Button, Description, Dropdown, Label } from "@heroui/react";
import type { DockerContainer } from "./docker-types";

export function ContainerMoreMenu({
  item,
  busy,
}: {
  item: DockerContainer;
  busy: boolean;
}) {
  return (
    <Dropdown>
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={`更多操作 ${item.name}`}
        isDisabled={busy || item.protected}
      >
        <Ellipsis aria-hidden="true" className="h-4 w-4" />
      </Button>
      <Dropdown.Popover placement="bottom end">
        <Dropdown.Menu aria-label={`${item.name}的更多操作`}>
          {[
            ["edit", "编辑"],
            ["upgrade", "升级"],
            ["backup", "备份"],
            ["commit", "制作镜像"],
            ["delete", "删除"],
          ].map(([id, label]) => (
            <Dropdown.Item
              key={id}
              id={id}
              isDisabled
              className={id === "delete" ? "text-danger" : undefined}
              textValue={label}
            >
              <Label>{label}</Label>
              <Description>暂未接入容器操作</Description>
            </Dropdown.Item>
          ))}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  );
}
