"use client";

import { ArrowDownToLine, Ellipsis, Eye, ShieldKeyhole } from "@gravity-ui/icons";
import { Button, Dropdown, Tooltip } from "@heroui/react";
import { useState } from "react";
import type { PlatformInstance } from "../../../../lib/types/platform-delivery";

export default function InstanceActions({
  item,
  busy,
  canEdit,
  onContainer,
  onTerminal,
  onDetail,
  onLogs,
  onDeploy,
  onEdit,
  onArchive,
  onVersions,
  onSupport,
  onDesktopDownload,
  desktopDownloading,
}: {
  item: PlatformInstance;
  busy: boolean;
  canEdit: boolean;
  onContainer: (logs: boolean) => void;
  onTerminal: () => void;
  onDetail: () => void;
  onLogs: () => void;
  onDeploy: () => void;
  onEdit: () => void;
  onArchive: () => void;
  onVersions: () => void;
  onSupport: () => void;
  onDesktopDownload: () => void;
  desktopDownloading: boolean;
}) {
  const [open, setOpen] = useState(false);
  const mutable =
    ["not_deployed", "failed"].includes(item.status) && !item.containerId;
  const action = (callback: () => void) => {
    setOpen(false);
    callback();
  };
  return (
    <div className="flex items-center justify-end gap-1">
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={`查看 ${item.name}`}
        onPress={onDetail}
      >
        <Eye className="h-4 w-4" />
      </Button>
      {item.containerId ? (
        <>
          <Tooltip>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label="下载桌面端安装包"
              isDisabled={desktopDownloading || item.status === "archived"}
              onPress={onDesktopDownload}
            >
              <ArrowDownToLine className="h-4 w-4" />
            </Button>
            <Tooltip.Content>下载桌面端安装包</Tooltip.Content>
          </Tooltip>
          <Tooltip>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label="平台运维访问"
              onPress={onSupport}
            >
              <ShieldKeyhole className="h-4 w-4" />
            </Button>
            <Tooltip.Content>平台运维访问</Tooltip.Content>
          </Tooltip>
        </>
      ) : null}
      <Dropdown isOpen={open} onOpenChange={setOpen}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={`更多操作 ${item.name}`}
        >
          <Ellipsis />
        </Button>
        <Dropdown.Popover placement="bottom end">
          <Dropdown.Menu aria-label={`${item.name}的更多操作`}>
            {item.containerId ? (
              <>
                <Dropdown.Item
                  id="versions"
                  isDisabled={busy}
                  onAction={() => action(onVersions)}
                >
                  版本与恢复点
                </Dropdown.Item>
                <Dropdown.Item
                  id="container-detail"
                  onAction={() => action(() => onContainer(false))}
                >
                  容器详情与启停
                </Dropdown.Item>
                <Dropdown.Item
                  id="container-terminal"
                  isDisabled={busy || item.runtime?.state !== "running"}
                  onAction={() => action(onTerminal)}
                >
                  容器终端
                </Dropdown.Item>
                <Dropdown.Item
                  id="container-log"
                  onAction={() => action(() => onContainer(true))}
                >
                  容器日志
                </Dropdown.Item>
              </>
            ) : null}
            <Dropdown.Item
              id="logs"
              aria-label={`部署记录 ${item.name}`}
              onAction={() => action(onLogs)}
            >
              部署记录
            </Dropdown.Item>
            {mutable ? (
              <>
                <Dropdown.Item
                  id="deploy"
                  aria-label={`部署 ${item.name}`}
                  isDisabled={busy}
                  onAction={() => action(onDeploy)}
                >
                  部署
                </Dropdown.Item>
                <Dropdown.Item
                  id="edit"
                  aria-label={`编辑 ${item.name}`}
                  isDisabled={!canEdit}
                  onAction={() => action(onEdit)}
                >
                  编辑
                </Dropdown.Item>
                <Dropdown.Item
                  id="archive"
                  aria-label={`归档 ${item.name}`}
                  className="text-danger"
                  onAction={() => action(onArchive)}
                >
                  归档
                </Dropdown.Item>
              </>
            ) : null}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
    </div>
  );
}
