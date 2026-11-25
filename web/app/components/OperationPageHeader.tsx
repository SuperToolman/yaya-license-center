"use client";
import { ArrowRotateRight } from "@gravity-ui/icons";
import { Button, Tooltip } from "@heroui/react";
export default function OperationPageHeader({
  eyebrow,
  title,
  description,
  loading,
  onRefresh,
  actions,
}: {
  eyebrow: string;
  title: string;
  description: string;
  loading?: boolean;
  onRefresh: () => void;
  actions?: React.ReactNode;
}) {
  return (
    <header className="mb-4 flex flex-wrap items-start justify-between gap-2">
      <div>
        <h1 className="text-2xl font-semibold">{title}</h1>
        <div className="flex items-center gap-2">
          <p className="font-bold text-accent">{eyebrow}</p>
          <p className="text-sm text-muted">{description}</p>
        </div>
      </div>
      <div className="flex items-center gap-2">
        <Tooltip>
          <Tooltip.Trigger>
            <Button
              isIconOnly
              variant="secondary"
              aria-label="刷新当前页面"
              isDisabled={loading}
              onPress={onRefresh}
            >
              <ArrowRotateRight
                className={`h-4 w-4 ${loading ? "animate-spin" : ""}`}
              />
            </Button>
          </Tooltip.Trigger>
          <Tooltip.Content>刷新</Tooltip.Content>
        </Tooltip>
        {actions}
      </div>
    </header>
  );
}
