"use client";

import { useEffect, useState } from "react";

const apiBase =
  process.env.NEXT_PUBLIC_OPERATION_API_BASE_URL ??
  `${typeof window === "undefined" ? "http://127.0.0.1:8779" : `${window.location.protocol}//${window.location.hostname}:8779`}`;
type Envelope<T> = { data: T | null; message: string };
type Order = {
  orderNo: string;
  customerName: string;
  totalAmountCents: number;
  receivedAmountCents: number;
  status: string;
};
type Commission = {
  entryId: string;
  orderId: string;
  productType: string;
  commissionAmountCents: number;
  status: string;
  createdAt: number;
};
type Settlement = {
  batchId: string;
  totalAmountCents: number;
  entryCount: number;
  status: string;
  settledAt: number;
  notes: string;
};
const money = (value: number) =>
  new Intl.NumberFormat("zh-CN", { style: "currency", currency: "CNY" }).format(
    value / 100,
  );
const time = (value: number) => new Date(value * 1000).toLocaleString("zh-CN");

export default function ProviderPortal() {
  const [orders, setOrders] = useState<Order[]>([]);
  const [commissions, setCommissions] = useState<Commission[]>([]);
  const [settlements, setSettlements] = useState<Settlement[]>([]);
  const [error, setError] = useState("");
  useEffect(() => {
    void (async () => {
      try {
        const get = async <T,>(path: string) =>
          (
            await fetch(`${apiBase}${path}`, { credentials: "include" })
          ).json() as Promise<Envelope<T>>;
        const [o, c, s] = await Promise.all([
          get<Order[]>("/api/orders"),
          get<Commission[]>("/api/commissions"),
          get<Settlement[]>("/api/settlement-batches"),
        ]);
        if (!o.data || !c.data || !s.data) {
          setError(o.message || c.message || s.message || "无法读取服务商数据");
          return;
        }
        setOrders(o.data);
        setCommissions(c.data);
        setSettlements(s.data);
      } catch {
        setError("请先使用服务商账号登录运营平台");
      }
    })();
  }, []);
  return (
    <main
      style={{
        maxWidth: 1180,
        margin: "0 auto",
        padding: "32px 20px",
        fontFamily: "Arial, sans-serif",
      }}
    >
      <header>
        <p style={{ color: "#0b7285", fontSize: 12 }}>PROVIDER PORTAL</p>
        <h1>服务商工作台</h1>
        <p>订单、返利和结算信息仅限当前服务商组织。</p>
      </header>
      {error ? (
        <p style={{ color: "#c92a2a" }}>{error}</p>
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
                    <td>{money(x.totalAmountCents)}</td>
                    <td>{money(x.receivedAmountCents)}</td>
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
                    <td>{money(x.commissionAmountCents)}</td>
                    <td>{x.status}</td>
                    <td>{time(x.createdAt)}</td>
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
                    <td>{money(x.totalAmountCents)}</td>
                    <td>{x.entryCount}</td>
                    <td>{x.status}</td>
                    <td>{time(x.settledAt)}</td>
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
