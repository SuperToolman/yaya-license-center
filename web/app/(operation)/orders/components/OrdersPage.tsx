"use client";
import { useOperation } from "../../../components/OperationProvider";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import { useOrders } from "../hooks/useOrders";

import {
  CircleCheck,
  CreditCard,
  Eye,
  Key,
  Pencil,
  Plus,
  TrashBin,
} from "@gravity-ui/icons";
import { Button, Input, Table } from "@heroui/react";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { ConfirmAction } from "../../../components/ui/ConfirmAction";
import { DataPanel } from "../../../components/ui/DataPanel";
import { DataTable } from "../../../components/ui/DataTable";
import { IconAction } from "../../../components/ui/IconAction";
import { SemanticChip } from "../../../components/ui/SemanticChip";
import { StatStrip } from "../../../components/ui/StatStrip";
import { formatDate, formatMoney } from "../../../lib/format";
import type { Order } from "../../../lib/operation-types";
import IssueLicenseModal from "./IssueLicenseModal";
import OrderDetailPanel from "./OrderDetailPanel";
import OrderModal from "./OrderModal";
import PaymentModal from "./PaymentModal";

const paymentStatuses = {
  unpaid: { label: "待付款", color: "warning" },
  partial: { label: "部分付款", color: "accent" },
  paid: { label: "已付款", color: "success" },
  cancelled: { label: "已取消", color: "danger" },
} as const;

const fulfillmentStatuses = {
  awaiting_payment: { label: "待付款", color: "warning" },
  pending: { label: "待履约", color: "warning" },
  completed: { label: "已完成", color: "success" },
  cancelled: { label: "已取消", color: "danger" },
} as const;

export default function OrdersPage({
  customerId,
}: { customerId?: string } = {}) {
  const { currentUser } = useOperation();
  const canFulfill = ["platform_admin", "platform_operator"].includes(
    currentUser?.role ?? "",
  );
  const {
    refresh,
    loading,
    error: queryError,
    orders,
    customers,
    employees,
    providers,
    licenses,
  } = useOrders();
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Order | null>(null);
  const [editor, setEditor] = useState<Order | null | undefined>(undefined);
  const [paymentOrder, setPaymentOrder] = useState<Order | null>(null);
  const [issueOrder, setIssueOrder] = useState<Order | null>(null);

  async function action(path: string, body?: unknown) {
    return mutation.run(() => operationCommand(path, body), refresh);
  }
  const scopedOrders = orders.filter(
    (o) => !customerId || o.customerId === customerId,
  );
  const visible = scopedOrders.filter((item) =>
    `${item.orderNo} ${item.customerName}`
      .toLowerCase()
      .includes(query.toLowerCase()),
  );

  return (
    <div className={customerId ? undefined : "flex min-h-0 flex-1 flex-col"}>
      <div className="shrink-0">
        {!customerId ? (
          <OperationPageHeader
            eyebrow="ORDER MANAGEMENT"
            title="订单中心"
            description="管理客户订单、回款状态和授权交付。"
            loading={loading || mutation.pending}
            onRefresh={refresh}
            actions={
              <Button onPress={() => setEditor(null)}>
                <Plus className="h-4 w-4" />
                新增订单
              </Button>
            }
          />
        ) : (
          <div className="mb-4 flex justify-end">
            <Button onPress={() => setEditor(null)}>新增订单</Button>
          </div>
        )}
        {error ? (
          <p role="alert" className="mb-4 text-sm text-danger">
            {error}
          </p>
        ) : null}
        {!customerId ? (
          <StatStrip
            items={[
              {
                label: "订单总额",
                value: formatMoney(
                  orders.reduce((sum, item) => sum + item.totalAmountCents, 0),
                ),
              },
              {
                label: "待回款",
                value: orders.filter(
                  (item) => item.status === "pending_payment",
                ).length,
              },
              {
                label: "待履约",
                value: orders.filter((item) => item.status === "paid").length,
              },
              {
                label: "已履约",
                value: orders.filter((item) => item.status === "fulfilled")
                  .length,
              },
            ]}
          />
        ) : null}
      </div>
      <DataPanel
        className={
          customerId
            ? undefined
            : "flex min-h-0 flex-1 flex-col overflow-hidden"
        }
        title="全部订单"
        note={`显示 ${visible.length} / ${orders.length} 笔订单`}
        actions={
          <Input
            aria-label="搜索订单号或客户"
            placeholder="搜索订单号或客户"
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
          />
        }
      >
        <DataTable
          label="订单列表"
          empty="暂无订单。"
          className={
            customerId ? undefined : "min-h-0 flex-1 grid-rows-[minmax(0,1fr)]"
          }
          scrollContainerClassName={
            customerId ? undefined : "min-h-0 overflow-auto"
          }
          columns={[
            "订单",
            "客户",
            "平台类型 / 回款",
            "金额 / 到账",
            "付款状态",
            "履约状态",
            "操作",
          ]}
        >
          {visible.map((item) => {
            const paymentStatus =
              paymentStatuses[
                item.paymentStatus ??
                  (item.status === "cancelled"
                    ? "cancelled"
                    : item.status === "pending_payment"
                      ? item.receivedAmountCents > 0
                        ? "partial"
                        : "unpaid"
                      : "paid")
              ];
            const fulfillmentStatus =
              fulfillmentStatuses[
                item.fulfillmentStatus ??
                  (item.status === "fulfilled"
                    ? "completed"
                    : item.status === "paid"
                      ? "pending"
                      : item.status === "cancelled"
                        ? "cancelled"
                        : "awaiting_payment")
              ];

            return (
              <Table.Row key={item.orderId} id={item.orderId}>
                <Table.Cell>
                  <span className="block font-medium">{item.orderNo}</span>
                  <small className="text-xs text-muted">
                    {formatDate(item.createdAt)}
                  </small>
                </Table.Cell>
                <Table.Cell>{item.customerName}</Table.Cell>
                <Table.Cell>
                  <SemanticChip
                    color={
                      item.deploymentType === "saas" ? "accent" : "warning"
                    }
                  >
                    {item.deploymentType === "saas" ? "SaaS" : "本地部署"}
                  </SemanticChip>
                  <small className="mt-1 block text-xs text-muted">
                    预计回款 {formatDate(item.dueAt)}
                  </small>
                </Table.Cell>
                <Table.Cell>
                  <span className="block">
                    {formatMoney(item.totalAmountCents)}
                  </span>
                  <small className="text-xs text-muted">
                    到账 {formatMoney(item.receivedAmountCents)}
                  </small>
                </Table.Cell>
                <Table.Cell>
                  <SemanticChip color={paymentStatus.color}>
                    {paymentStatus.label}
                  </SemanticChip>
                </Table.Cell>
                <Table.Cell>
                  <SemanticChip color={fulfillmentStatus.color}>
                    {fulfillmentStatus.label}
                  </SemanticChip>
                </Table.Cell>
                <Table.Cell>
                  <div className="flex items-center gap-1 whitespace-nowrap">
                    <IconAction
                      label="查看订单详情"
                      onPress={() => setSelected(item)}
                    >
                      <Eye />
                    </IconAction>
                    {item.status === "pending_payment" && !item.items.some(i => i.kind === "application") ? (
                      <>
                        <IconAction
                          label="编辑订单"
                          onPress={() => setEditor(item)}
                        >
                          <Pencil />
                        </IconAction>
                        <IconAction
                          label="登记回款"
                          onPress={() => setPaymentOrder(item)}
                        >
                          <CreditCard />
                        </IconAction>
                        {item.receivedAmountCents === 0 ? (
                          <ConfirmAction
                            label="取消订单"
                            danger
                            isIconOnly
                            onConfirm={() =>
                              action(`/api/orders/${item.orderId}/cancel`)
                            }
                          >
                            <TrashBin />
                          </ConfirmAction>
                        ) : null}
                      </>
                    ) : null}
                    {item.status === "paid" && canFulfill ? (
                      <IconAction
                        label={
                          item.items.every((i) => i.kind === "service")
                            ? "确认服务完成"
                            : "交付授权"
                        }
                        onPress={() =>
                          item.items.every((i) => i.kind === "service")
                            ? void action(
                                `/api/orders/${item.orderId}/fulfill-service`,
                              )
                            : setIssueOrder(item)
                        }
                      >
                        {item.items.every((i) => i.kind === "service") ? (
                          <CircleCheck />
                        ) : (
                          <Key />
                        )}
                      </IconAction>
                    ) : null}
                  </div>
                </Table.Cell>
              </Table.Row>
            );
          })}
        </DataTable>
      </DataPanel>
      {selected ? (
        <OrderDetailPanel
          order={selected}
          license={licenses.find(
            (license) => license.orderId === selected.orderId,
          )}
          onClose={() => setSelected(null)}
        />
      ) : null}
      {editor !== undefined ? (
        <OrderModal
          order={editor}
          customerId={customerId}
          customers={customers}
          employees={employees}
          providers={providers}
          onClose={() => setEditor(undefined)}
          onSaved={refresh}
        />
      ) : null}
      {paymentOrder ? (
        <PaymentModal
          order={paymentOrder}
          onClose={() => setPaymentOrder(null)}
          onSaved={refresh}
        />
      ) : null}
      {issueOrder ? (
        <IssueLicenseModal
          order={issueOrder}
          onClose={() => setIssueOrder(null)}
          onSaved={refresh}
        />
      ) : null}
    </div>
  );
}
