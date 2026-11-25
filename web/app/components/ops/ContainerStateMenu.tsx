"use client";
import { Button, Dropdown } from "@heroui/react";
import { useState } from "react";
import {
  actionLabels,
  stateLabels,
  type ContainerAction,
  type DockerContainer,
} from "./docker-types";
export function ContainerStateMenu({
  item,
  busy,
  onAction,
}: {
  item: DockerContainer;
  busy: boolean;
  onAction: (action: ContainerAction) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dropdown isOpen={open} onOpenChange={setOpen}>
      <Button
        size="sm"
        variant="secondary"
        aria-label={`状态操作 ${item.name}`}
        isDisabled={busy || item.protected}
        className={
          item.state === "running"
            ? "text-success"
            : item.state === "paused"
              ? "text-warning"
              : "text-muted"
        }
      >
        {stateLabels[item.state] ?? item.state}{" "}
        {item.protected ? "（受保护）" : "▾"}
      </Button>
      <Dropdown.Popover placement="bottom start">
        <Dropdown.Menu aria-label={`${item.name}的状态操作`}>
          {(Object.keys(actionLabels) as ContainerAction[]).map((action) => (
            <Dropdown.Item
              key={action}
              id={action}
              isDisabled={!item.allowedActions.includes(action)}
              className={action === "kill" ? "text-danger" : ""}
              onAction={() => {
                setOpen(false);
                onAction(action);
              }}
            >
              {actionLabels[action]}
            </Dropdown.Item>
          ))}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  );
}
