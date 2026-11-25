"use client";
import { Eye } from "@gravity-ui/icons";
import {
  Button,
  Chip,
  DateField,
  DateRangePicker,
  Label,
  ListBox,
  RangeCalendar,
  SearchField,
  Select,
  Spinner,
  Table,
  TextArea,
} from "@heroui/react";
import { getLocalTimeZone, type CalendarDate } from "@internationalized/date";
import { useEffect, useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { DataPanel } from "../../../components/ui/DataPanel";
import { IconAction } from "../../../components/ui/IconAction";
import { ModalShell } from "../../../components/ui/ModalShell";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { useAuditLogs } from "./useAuditLogs";
import { formatDate } from "../../../lib/format";
import type { AuditLog, Customer } from "../../../lib/operation-types";

const levels = {
  debug: { label: "调试", color: "default" },
  info: { label: "信息", color: "accent" },
  warning: { label: "警告", color: "warning" },
  error: { label: "错误", color: "danger" },
} as const;

export default function LogsPage() {
  const customers = useData<Customer[]>("/api/customers");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState("");
  const [level, setLevel] = useState("all");
  const [customerId, setCustomerId] = useState("all");
  const [dateRange, setDateRange] = useState<{
    start: CalendarDate;
    end: CalendarDate;
  } | null>(null);
  const [selected, setSelected] = useState<AuditLog | null>(null);
  useEffect(() => {
    const timer = setTimeout(() => setSearch(query.trim()), 300);
    return () => clearTimeout(timer);
  }, [query]);
  const invalidDates = Boolean(
    dateRange && dateRange.start.compare(dateRange.end) > 0,
  );
  const params = new URLSearchParams();
  if (level !== "all") params.set("level", level);
  if (customerId !== "all") params.set("customerId", customerId);
  if (search) params.set("search", search);
  if (dateRange && !invalidDates) {
    params.set(
      "start",
      String(
        Math.floor(dateRange.start.toDate(getLocalTimeZone()).getTime() / 1000),
      ),
    );
    params.set(
      "end",
      String(
        Math.floor(
          dateRange.end.add({ days: 1 }).toDate(getLocalTimeZone()).getTime() /
            1000,
        ),
      ),
    );
  }
  const logs = useAuditLogs(params.toString(), !invalidDates);
  function resetFilters() {
    setLevel("all");
    setCustomerId("all");
    setDateRange(null);
    setQuery("");
    setSearch("");
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="AUDIT LOGS"
        title="日志信息"
        description="审查运营端请求来源、接口状态和风险级别。"
        loading={logs.loading}
        onRefresh={logs.refresh}
      />
      <DataPanel title="运营审计日志">
        <div className="grid items-end gap-3 border-b border-border p-4 sm:grid-cols-2 xl:grid-cols-[minmax(280px,1.6fr)_minmax(110px,.65fr)_minmax(150px,1fr)_minmax(180px,1.2fr)_auto]">
          <DateRangePicker
            aria-label="日志日期范围"
            value={dateRange}
            onChange={setDateRange}
            className="min-w-0"
            isInvalid={invalidDates}
          >
            <Label>日志日期范围</Label>
            <DateField.Group>
              <DateField.Input slot="start" aria-label="开始日期">
                {(segment) => <DateField.Segment segment={segment} />}
              </DateField.Input>
              <DateRangePicker.RangeSeparator />
              <DateField.Input slot="end" aria-label="结束日期">
                {(segment) => <DateField.Segment segment={segment} />}
              </DateField.Input>
              <DateField.Suffix>
                <DateRangePicker.Trigger aria-label="选择日志日期范围">
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
          <Select
            aria-label="级别筛选"
            selectedKey={level}
            onSelectionChange={(key) => setLevel(String(key))}
            className="min-w-0"
          >
            <Label>日志级别</Label>
            <Select.Trigger>
              <Select.Value />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                <ListBox.Item id="all">全部级别</ListBox.Item>
                {Object.entries(levels).map(([id, value]) => (
                  <ListBox.Item key={id} id={id} textValue={value.label}>
                    <Chip
                      size="sm"
                      variant="soft"
                      color={value.color}
                      className={
                        id === "debug"
                          ? "!bg-zinc-200 !text-zinc-700 dark:!bg-zinc-700 dark:!text-zinc-100"
                          : undefined
                      }
                    >
                      {value.label}
                    </Chip>
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
          <Select
            aria-label="客户筛选"
            selectedKey={customerId}
            onSelectionChange={(key) => setCustomerId(String(key))}
            className="min-w-0"
          >
            <Label>客户</Label>
            <Select.Trigger>
              <Select.Value />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                <ListBox.Item id="all">全部客户</ListBox.Item>
                <ListBox.Item id="__none__">无客户</ListBox.Item>
                {(customers.data ?? []).map((customer) => (
                  <ListBox.Item
                    key={customer.customerId}
                    id={customer.customerId}
                    textValue={customer.name}
                  >
                    {customer.name}
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
          <SearchField
            aria-label="搜索日志"
            value={query}
            onChange={setQuery}
            className="min-w-0"
          >
            <Label>搜索日志</Label>
            <SearchField.Group>
              <SearchField.SearchIcon />
              <SearchField.Input placeholder="客户、接口、状态码或 IP" />
              <SearchField.ClearButton aria-label="清空搜索" />
            </SearchField.Group>
          </SearchField>
          <Button variant="secondary" onPress={resetFilters}>
            重置筛选
          </Button>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-3 text-xs text-muted">
          <span>
            已加载 {logs.items.length} 条
            {logs.hasMore
              ? " · 向下滚动加载更多"
              : logs.loading
                ? " · 正在加载…"
                : " · 已显示全部匹配日志"}
          </span>
          <span>按时间从新到旧</span>
        </div>
        {invalidDates ? (
          <p role="alert" className="px-4 pb-3 text-sm text-danger">
            开始日期不能晚于结束日期。
          </p>
        ) : null}
        {customers.error ? (
          <p role="alert" className="px-4 pb-3 text-sm text-danger">
            客户选项读取失败：{customers.error}
          </p>
        ) : null}
        {logs.error ? (
          <div
            role="alert"
            className="flex items-center gap-3 px-4 pb-3 text-sm text-danger"
          >
            {logs.error}
            <Button
              size="sm"
              variant="secondary"
              onPress={logs.items.length ? logs.loadMore : logs.refresh}
            >
              重试
            </Button>
          </div>
        ) : null}
        <Table>
          <Table.ScrollContainer
            key={params.toString()}
            className="h-[560px] max-h-[65dvh] overflow-auto overscroll-contain"
          >
            <Table.Content aria-label="运营审计日志" className="min-w-[780px]">
              <Table.Header className="sticky top-0 z-10 bg-surface">
                {["时间", "级别", "来源", "客户", "结果", "操作"].map(
                  (column, index) => (
                    <Table.Column
                      key={column}
                      id={column}
                      isRowHeader={index === 0}
                    >
                      {column}
                    </Table.Column>
                  ),
                )}
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="p-10 text-center text-sm text-muted">
                    {invalidDates
                      ? "请选择有效的日期范围。"
                      : logs.error
                        ? "日志读取失败，请重试。"
                        : logs.loading
                          ? "正在加载日志…"
                          : "当前筛选条件下没有日志。"}
                  </div>
                )}
              >
                <Table.Collection items={logs.items}>
                  {(item) => (
                    <Table.Row key={item.id} id={String(item.id)}>
                      <Table.Cell>{formatDate(item.createdAt)}</Table.Cell>
                      <Table.Cell>
                        <Chip
                          size="sm"
                          variant="soft"
                          color={levels[item.level]?.color ?? "default"}
                          className={
                            item.level === "debug"
                              ? "!bg-zinc-200 !text-zinc-700 dark:!bg-zinc-700 dark:!text-zinc-100"
                              : undefined
                          }
                        >
                          {levels[item.level]?.label ?? item.level}
                        </Chip>
                      </Table.Cell>
                      <Table.Cell>
                        {item.endpointTitle || item.source}
                      </Table.Cell>
                      <Table.Cell>{item.customerName || "无客户"}</Table.Cell>
                      <Table.Cell>{item.statusCode || "-"}</Table.Cell>
                      <Table.Cell>
                        <IconAction
                          label="查看日志详情"
                          onPress={() => setSelected(item)}
                        >
                          <Eye />
                        </IconAction>
                      </Table.Cell>
                    </Table.Row>
                  )}
                </Table.Collection>
                {logs.hasMore && !logs.error ? (
                  <Table.LoadMore
                    isLoading={logs.loading}
                    scrollOffset={0}
                    onLoadMore={logs.loadMore}
                  >
                    <Table.LoadMoreContent>
                      <Spinner size="sm" />
                      <span className="ml-2 text-sm text-muted">
                        正在加载更多日志…
                      </span>
                    </Table.LoadMoreContent>
                  </Table.LoadMore>
                ) : null}
              </Table.Body>
            </Table.Content>
          </Table.ScrollContainer>
        </Table>
      </DataPanel>
      {selected ? (
        <ModalShell title="日志信息" onClose={() => setSelected(null)}>
          <p className="mb-3">{selected.message}</p>
          <TextArea
            aria-label="返回结果"
            value={selected.responseBody || "未记录响应正文"}
            readOnly
          />
        </ModalShell>
      ) : null}
    </>
  );
}
