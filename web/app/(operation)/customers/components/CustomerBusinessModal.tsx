"use client";
import { Button, Tabs } from "@heroui/react";
import { Pencil } from "@gravity-ui/icons";
import { CustomerFields } from "./CustomerFields";
import { useState } from "react";
import { ModalShell } from "../../../components/ui/ModalShell";
import { useOperation } from "../../../components/OperationProvider";
import type { Customer } from "../../../lib/operation-types";
import OrdersPage from "../../orders/components/OrdersPage";
import CustomerPlatformInstances from "../platform-instances/components/CustomerPlatformInstances";
import CustomerAuthorizationPanel from "../../../components/CustomerAuthorizationPanel";
export default function CustomerBusinessModal({
  customer,
  initialTab,
  onClose,
  onEdit,
}: {
  customer: Customer;
  initialTab: string;
  onClose: () => void;
  onEdit: () => void;
}) {
  const [tab, setTab] = useState(initialTab);
  const { currentUser } = useOperation();
  return (
    <ModalShell
      title={`${customer.name} · 客户业务`}
      width={1440}
      height={800}
      bodyClassName="flex min-h-0 flex-1 flex-col overflow-hidden p-5"
      onClose={onClose}
    >
      <Tabs
        className="min-h-0 flex-1"
        selectedKey={tab}
        onSelectionChange={(key) => setTab(String(key))}
      >
        <Tabs.ListContainer className="shrink-0 overflow-x-auto">
          <Tabs.List aria-label="客户业务">
            <Tabs.Tab id="overview">
              概览
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="orders">
              订单
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="authorization">
              授权
              <Tabs.Indicator />
            </Tabs.Tab>
            {currentUser?.role === "platform_admin" ? (
              <Tabs.Tab id="instances">
                平台实例
                <Tabs.Indicator />
              </Tabs.Tab>
            ) : null}
          </Tabs.List>
        </Tabs.ListContainer>
        <Tabs.Panel
          id={tab}
          className="min-h-0 flex-1 overflow-y-auto pt-4 [scrollbar-gutter:stable]"
        >
          {tab === "overview" ? (
            <div className="space-y-5">
              <div className="flex items-center justify-between gap-3">
                <p className="text-sm text-muted">客户基本资料和联系方式。</p>
                <Button variant="secondary" onPress={onEdit}>
                  <Pencil aria-hidden="true" />
                  编辑客户
                </Button>
              </div>
              <CustomerFields
                readOnly
                form={{
                  name: customer.name,
                  contactName: customer.contactName || "",
                  contactPhone: customer.contactPhone || "",
                  contactEmail: customer.contactEmail || "",
                  notes: customer.notes || "",
                }}
              />
            </div>
          ) : null}
          {tab === "orders" ? (
            <OrdersPage customerId={customer.customerId} />
          ) : null}
          {tab === "authorization" ? (
            <CustomerAuthorizationPanel customerId={customer.customerId} />
          ) : null}
          {tab === "instances" && currentUser?.role === "platform_admin" ? (
            <CustomerPlatformInstances customer={customer} />
          ) : null}
        </Tabs.Panel>
      </Tabs>
    </ModalShell>
  );
}
