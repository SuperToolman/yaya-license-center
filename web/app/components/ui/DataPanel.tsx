"use client";
import { Card } from "@heroui/react";
import { type ReactNode } from "react";
export function DataPanel({
  title,
  note,
  children,
  actions,
  className = "mb-4",
}: {
  title: string;
  note?: string;
  children: ReactNode;
  actions?: ReactNode;
  className?: string;
}) {
  return (
    <Card className={className}>
      {/* <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 px-4 py-3">
        <div>
          <h2 className="text-sm font-semibold">{title}</h2>
          {note ? <p className="mt-0.5 text-xs text-muted">{note}</p> : null}
        </div>
        {actions}
      </div> */}
      {children}
    </Card>
  );
}
