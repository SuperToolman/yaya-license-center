"use client";
import { Alert, Label } from "@heroui/react";
import { useOperationQuery } from "../../hooks/useOperationQuery";
import { formatDate, formatMoney } from "../../lib/format";
import { operationData } from "../../lib/operation-api";
import type { CommissionEntry, Order } from "../../lib/operation-types";
import type { Settlement } from "../../lib/types/providers";
export default function ProviderPortal() {
  const { data, error, loading } = useOperationQuery(
    "provider-portal",
    async (signal) => {
      const [orders, commissions, settlements] = await Promise.all([
        operationData<Order[]>("/api/orders", { signal }),
        operationData<CommissionEntry[]>("/api/commissions", { signal }),
        operationData<Settlement[]>("/api/settlement-batches", { signal }),
      ]);
      return { orders, commissions, settlements };
    },
  );
  const { orders = [], commissions = [], settlements = [] } = data ?? {};
  return (
    <main className="mx-auto max-w-[1180px] px-5 py-8">
      <header>
        <Label>PROVIDER PORTAL</Label>
        <h1>服务商工作台</h1>
        <p>订单、返利和结算信息仅限当前服务商组织。</p>
      </header>
      {loading ? <p>正在读取服务商数据...</p> : null}
      {error ? (
        <Alert status="danger">
          <Alert.Content>
            <Alert.Description>{error}</Alert.Description>
          </Alert.Content>
        </Alert>
      ) : (
        <>
          <section>
            <h2>我的订单</h2>
            <table>
              <thead>
                <tr>
                  <th>订单</th>
                  <th>客户</th>
                  <th>订单金额</th>
                  <th>已回款</th>
                  <th>状态</th>
                </tr>
              </thead>
              <tbody>
                {orders.map((x) => (
                  <tr key={x.orderNo}>
                    <td>{x.orderNo}</td>
                    <td>{x.customerName}</td>
                    <td>{formatMoney(x.totalAmountCents)}</td>
                    <td>{formatMoney(x.receivedAmountCents)}</td>
                    <td>{x.status}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
          <section>
            <h2>我的返利</h2>
            <table>
              <thead>
                <tr>
                  <th>订单</th>
                  <th>产品</th>
                  <th>返利</th>
                  <th>状态</th>
                  <th>时间</th>
                </tr>
              </thead>
              <tbody>
                {commissions.map((x) => (
                  <tr key={x.entryId}>
                    <td>{x.orderId}</td>
                    <td>{x.productType}</td>
                    <td>{formatMoney(x.commissionAmountCents)}</td>
                    <td>{x.status}</td>
                    <td>{formatDate(x.createdAt)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
          <section>
            <h2>结算记录</h2>
            <table>
              <thead>
                <tr>
                  <th>批次</th>
                  <th>金额</th>
                  <th>笔数</th>
                  <th>状态</th>
                  <th>结算时间</th>
                </tr>
              </thead>
              <tbody>
                {settlements.map((x) => (
                  <tr key={x.batchId}>
                    <td>{x.batchId}</td>
                    <td>{formatMoney(x.totalAmountCents)}</td>
                    <td>{x.entryCount}</td>
                    <td>{x.status}</td>
                    <td>{formatDate(x.settledAt)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        </>
      )}
    </main>
  );
}
