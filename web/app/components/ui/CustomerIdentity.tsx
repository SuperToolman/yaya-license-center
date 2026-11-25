"use client";

import { Copy } from "@gravity-ui/icons";
import { Button, toast } from "@heroui/react";

export function CustomerIdentity({
  customerId,
  name,
}: {
  customerId: string;
  name?: string;
}) {
  return (
    <div className="min-w-0">
      <span className="break-words font-medium">{name || "未关联客户"}</span>
      {customerId ? (
        <div className="flex items-center text-xs text-muted">
          <span className="min-w-0 break-all select-text">{customerId}</span>
        </div>
      ) : (
        <p className="mt-1 text-xs text-muted">客户 ID 缺失</p>
      )}
    </div>
  );
}
