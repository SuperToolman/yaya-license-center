"use client";
import { Table } from "@heroui/react";
import { type ReactNode } from "react";
export function DataTable({
  label,
  columns,
  children,
  empty,
  columnWidths,
  contentClassName = "min-w-[720px]",
  scrollContainerClassName = "overflow-x-auto",
  className,
}: {
  label: string;
  columns: string[];
  children: ReactNode;
  empty: string;
  columnWidths?: Record<string, number | string>;
  contentClassName?: string;
  scrollContainerClassName?: string;
  className?: string;
}) {
  return (
    <Table className={className}>
      <Table.ScrollContainer className={scrollContainerClassName}>
        <Table.Content aria-label={label} className={contentClassName}>
          <Table.Header>
            {columns.map((column, index) => (
              <Table.Column
                key={column}
                id={column}
                isRowHeader={index === 0}
                style={{ width: columnWidths?.[column] }}
              >
                {column}
              </Table.Column>
            ))}
          </Table.Header>
          <Table.Body
            renderEmptyState={() => (
              <div className="p-10 text-center text-sm text-muted">{empty}</div>
            )}
          >
            {children}
          </Table.Body>
        </Table.Content>
      </Table.ScrollContainer>
    </Table>
  );
}
