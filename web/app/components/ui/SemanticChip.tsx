"use client";

import { Chip } from "@heroui/react";
import type { ReactNode } from "react";

export type ChipColor = "accent" | "success" | "warning" | "danger" | "default";

export function SemanticChip({
  children,
  color = "default",
  className = "",
}: {
  children: ReactNode;
  color?: ChipColor;
  className?: string;
}) {
  return (
    <Chip size="sm" color={color} variant="soft" className={className}>
      {children}
    </Chip>
  );
}

const statuses: Record<string, { label: string; color: ChipColor }> = {
  active: { label: "启用", color: "success" },
  inactive: { label: "停用", color: "default" },
  pending_payment: { label: "待回款", color: "warning" },
  paid: { label: "已回款", color: "success" },
  fulfilled: { label: "已交付", color: "success" },
  cancelled: { label: "已取消", color: "default" },
  running: { label: "运行中", color: "success" },
  unactivated: { label: "未激活", color: "warning" },
  expired: { label: "已过期", color: "danger" },
  destroyed: { label: "已销毁", color: "danger" },
  approved: { label: "已通过", color: "success" },
  pending_review: { label: "待审核", color: "default" },
  rejected: { label: "已驳回", color: "danger" },
  legacy_unlinked: { label: "历史未关联", color: "default" },
};

export function StatusChip({
  status,
  label,
}: {
  status: string;
  label?: string;
}) {
  const entry = statuses[status];
  return (
    <SemanticChip color={entry?.color ?? "default"}>
      {label ?? entry?.label ?? status}
    </SemanticChip>
  );
}
