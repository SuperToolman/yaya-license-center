"use client";
import {
  Button,
  RangeCalendar,
  DateField,
  DateRangePicker,
  Label,
  ListBox,
  SearchField,
  Select,
  Table,
} from "@heroui/react";
import { getLocalTimeZone, type CalendarDate } from "@internationalized/date";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { DataPanel } from "../../../components/ui/DataPanel";
import { DataTable } from "../../../components/ui/DataTable";
import { StatStrip } from "../../../components/ui/StatStrip";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { formatDate, formatMoney } from "../../../lib/format";
import type { FinanceSummary, Transaction } from "../../../lib/operation-types";

export default function FinancePage() {
  const summary = useData<FinanceSummary>("/api/finance/summary");
  const transactions = useData<Transaction[]>("/api/transactions");
  const items = transactions.data || [];
  const [customerId, setCustomerId] = useState("all");
  const [query, setQuery] = useState("");
  const [method, setMethod] = useState("all");
  const [dateRange, setDateRange] = useState<{
    start: CalendarDate;
    end: CalendarDate;
  } | null>(null);
  const customers = Array.from(
    new Map(items.map((item) => [item.customerId, item.customerName])),
  );
  const methods = Array.from(new Set(items.map((item) => item.method)));
  const invalidDates = Boolean(
    dateRange && dateRange.start.compare(dateRange.end) > 0,
  );
  const start = dateRange
    ? dateRange.start.toDate(getLocalTimeZone()).getTime() / 1000
    : -Infinity;
  const end = dateRange
    ? (dateRange.end.add({ days: 1 }).toDate(getLocalTimeZone()).getTime() -
        1) /
      1000
    : Infinity;
  const visible = items.filter(
    (item) =>
      !invalidDates &&
      (customerId === "all" || item.customerId === customerId) &&
      (method === "all" || item.method === method) &&
      item.occurredAt >= start &&
      item.occurredAt <= end &&
      [item.customerName, item.orderNo, item.reference, item.notes, item.method]
        .join(" ")
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
  );
  const filteredTotal = visible.reduce(
    (total, item) => total + item.amountCents,
    0,
  );
  function resetFilters() {
    setCustomerId("all");
    setQuery("");
    setMethod("all");
    setDateRange(null);
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="FINANCE MANAGEMENT"
        title="财务管理"
        description="查看经营回款、待收金额和收款流水。"
        loading={summary.loading || transactions.loading}
        onRefresh={summary.refresh}
      />
      {summary.error || transactions.error ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {summary.error || transactions.error}
        </p>
      ) : null}
      <StatStrip
        items={[
          {
            label: "累计回款",
            value: formatMoney(summary.data?.totalReceivedCents),
          },
          {
            label: "本月回款",
            value: formatMoney(summary.data?.monthReceivedCents),
          },
          {
            label: "待收金额",
            value: formatMoney(summary.data?.outstandingCents),
          },
          { label: "已回款订单", value: summary.data?.paidOrderCount || 0 },
        ]}
      />
      <DataPanel
        title="收款流水"
        note={`显示 ${visible.length} / ${items.length} 笔流水，筛选合计 ${formatMoney(filteredTotal)}`}
      >
        <div className="grid items-end gap-3 border-b border-border p-4 sm:grid-cols-2 xl:grid-cols-[minmax(120px,1fr)_minmax(120px,1fr)_minmax(280px,1.7fr)_minmax(200px,1.5fr)_auto]">
          <div className="min-w-0">
            <Select
              aria-label="客户筛选"
              selectedKey={customerId}
              onSelectionChange={(key) => setCustomerId(String(key))}
              className="w-full"
            >
              <Label>客户筛选</Label>
              <Select.Trigger>
                <Select.Value />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  <ListBox.Item id="all">全部客户</ListBox.Item>
                  {customers.map(([id, name]) => (
                    <ListBox.Item key={id} id={id}>
                      {name}
                    </ListBox.Item>
                  ))}
                </ListBox>
              </Select.Popover>
            </Select>
          </div>
          <div className="min-w-0">
            <Select
              aria-label="收款方式"
              selectedKey={method}
              onSelectionChange={(key) => setMethod(String(key))}
              className="w-full"
            >
              <Label>收款方式</Label>
              <Select.Trigger>
                <Select.Value />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  <ListBox.Item id="all">全部方式</ListBox.Item>
                  {methods.map((value) => (
                    <ListBox.Item key={value} id={value}>
                      {value || "未记录"}
                    </ListBox.Item>
                  ))}
                </ListBox>
              </Select.Popover>
            </Select>
          </div>
          <DateRangePicker
            aria-label="收款日期范围"
            value={dateRange}
            onChange={setDateRange}
            className="min-w-0"
            isInvalid={invalidDates}
          >
            <Label>收款日期范围</Label>
            <DateField.Group>
              <DateField.Input slot="start" aria-label="开始日期">
                {(segment) => <DateField.Segment segment={segment} />}
              </DateField.Input>
              <DateRangePicker.RangeSeparator />
              <DateField.Input slot="end" aria-label="结束日期">
                {(segment) => <DateField.Segment segment={segment} />}
              </DateField.Input>
              <DateField.Suffix>
                <DateRangePicker.Trigger aria-label="选择收款日期范围">
                  <DateRangePicker.TriggerIndicator />
                </DateRangePicker.Trigger>
              </DateField.Suffix>
            </DateField.Group>
            <DateRangePicker.Popover>
              <RangeCalendar>
                <RangeCalendar.Header>
                  <RangeCalendar.NavButton slot="previous" />
                  <RangeCalendar.Heading />
                  <RangeCalendar.NavButton slot="next" />
                </RangeCalendar.Header>
                <RangeCalendar.Grid>
                  <RangeCalendar.GridHeader>
                    {(day) => (
                      <RangeCalendar.HeaderCell>{day}</RangeCalendar.HeaderCell>
                    )}
                  </RangeCalendar.GridHeader>
                  <RangeCalendar.GridBody>
                    {(date) => <RangeCalendar.Cell date={date} />}
                  </RangeCalendar.GridBody>
                </RangeCalendar.Grid>
              </RangeCalendar>
            </DateRangePicker.Popover>
          </DateRangePicker>
          <SearchField
            aria-label="搜索流水"
            value={query}
            onChange={setQuery}
            className="min-w-0"
          >
            <Label>搜索流水</Label>
            <SearchField.Group>
              <SearchField.SearchIcon />
              <SearchField.Input placeholder="客户、订单号、流水号或备注" />
              <SearchField.ClearButton aria-label="清空搜索" />
            </SearchField.Group>
          </SearchField>
          <div className="flex items-end">
            <Button variant="secondary" onPress={resetFilters}>
              重置筛选
            </Button>
          </div>
          {invalidDates ? (
            <p
              role="alert"
              className="text-sm text-danger sm:col-span-2 xl:col-span-5"
            >
              开始日期不能晚于结束日期。
            </p>
          ) : null}
        </div>
        <p className="px-4 py-3 text-xs text-muted">
          显示 {visible.length} / {items.length} 笔流水，筛选合计{" "}
          {formatMoney(filteredTotal)}
        </p>
        <DataTable
          label="收款流水"
          empty={
            items.length ? "暂无符合筛选条件的收款流水。" : "暂无收款流水。"
          }
          columns={["时间", "客户", "订单", "方式", "流水号", "金额"]}
        >
          {visible.map((item) => (
            <Table.Row key={item.transactionId} id={item.transactionId}>
              <Table.Cell>{formatDate(item.occurredAt)}</Table.Cell>
              <Table.Cell>{item.customerName}</Table.Cell>
              <Table.Cell>{item.orderNo}</Table.Cell>
              <Table.Cell>{item.method}</Table.Cell>
              <Table.Cell>{item.reference || "-"}</Table.Cell>
              <Table.Cell>{formatMoney(item.amountCents)}</Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
    </>
  );
}
