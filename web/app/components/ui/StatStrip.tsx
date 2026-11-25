"use client";
import { Card } from "@heroui/react";
import { type ReactNode } from "react";
export function StatStrip({
  items,
}: {
  items: Array<{ label: string; value: ReactNode }>;
}) {
  return (
    <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
      {items.map((item) => (
        <Card key={item.label} className="p-4">
          <span className="text-xs text-muted">{item.label}</span>
          <strong className="mt-2 block text-xl">{item.value}</strong>
        </Card>
      ))}
    </div>
  );
}
