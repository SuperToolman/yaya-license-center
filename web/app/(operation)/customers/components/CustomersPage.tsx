"use client";
import {
  Ellipsis,
  Eye,
  CirclePlayFill,
  CircleStop,
  Plus,
} from "@gravity-ui/icons";
import { Button, Dropdown, Table, Tooltip } from "@heroui/react";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { DataPanel } from "../../../components/ui/DataPanel";
import { CustomerIdentity } from "../../../components/ui/CustomerIdentity";
import { DataTable } from "../../../components/ui/DataTable";
import { IconAction } from "../../../components/ui/IconAction";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { StatStrip } from "../../../components/ui/StatStrip";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import type { Customer } from "../../../lib/operation-types";
import { useCustomers } from "../hooks/useCustomers";
import CustomerModal from "./CustomerModal";
import DeleteCustomerDialog from "./DeleteCustomerDialog";
import CustomerBusinessModal from "./CustomerBusinessModal";
export default function CustomersPage() {
  const {
    refresh,
    loading,
    error: queryError,
    customers,
    orders,
  } = useCustomers();
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [selected, setSelected] = useState<Customer | null | undefined>(
    undefined,
  );
  const [instanceCustomer, setInstanceCustomer] = useState<Customer | null>(
    null,
  );
  const [deleting, setDeleting] = useState<Customer | null>(null);
  async function toggle(item: Customer) {
    await mutation.run(
      () =>
        operationCommand(
          `/api/customers/${encodeURIComponent(item.customerId)}/status`,
          { status: item.status === "active" ? "inactive" : "active" },
        ),
      refresh,
    );
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="CUSTOMER MANAGEMENT"
        title="客户管理"
        description="沉淀客户档案、联系人和合作状态。"
        loading={loading || mutation.pending}
        onRefresh={refresh}
        actions={
          <Button onPress={() => setSelected(null)}>
            <Plus className="h-4 w-4" />
            新增客户
          </Button>
        }
      />
      {error ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {error}
        </p>
      ) : null}
      <StatStrip
        items={[
          { label: "客户总数", value: customers.length },
          {
            label: "合作中",
            value: customers.filter((item) => item.status === "active").length,
          },
          {
            label: "已停用",
            value: customers.filter((item) => item.status === "inactive")
              .length,
          },
          { label: "关联订单", value: orders.length },
        ]}
      />
      <DataPanel title="客户档案" note={`${customers.length} 家客户`}>
        <DataTable
          label="客户档案"
          empty="暂无客户，请先创建客户档案。"
          contentClassName="min-w-[1100px] table-fixed"
          columnWidths={{
            联系人: 110,
            联系电话: 160,
            联系邮箱: 220,
            订单: 80,
            状态: 90,
            操作: 144,
          }}
          columns={[
            "客户",
            "联系人",
            "联系电话",
            "联系邮箱",
            "订单",
            "状态",
            "操作",
          ]}
        >
          {customers.map((item) => (
            <Table.Row key={item.customerId} id={item.customerId}>
              <Table.Cell>
                <CustomerIdentity
                  name={item.name}
                  customerId={item.customerId}
                />
              </Table.Cell>
              <Table.Cell>{item.contactName || "-"}</Table.Cell>
              <Table.Cell>{item.contactPhone || "-"}</Table.Cell>
              <Table.Cell>
                <span className="break-all">{item.contactEmail || "-"}</span>
              </Table.Cell>
              <Table.Cell>
                {
                  orders.filter((order) => order.customerId === item.customerId)
                    .length
                }{" "}
                笔
              </Table.Cell>
              <Table.Cell>
                <StatusChip status={item.status} />
              </Table.Cell>
              <Table.Cell>
                <div className="flex items-center gap-1 whitespace-nowrap">
                  <Tooltip>
                    <Button
                      isIconOnly
                      size="sm"
                      variant="ghost"
                      aria-label={`查看详情 ${item.name}`}
                      onPress={() => setInstanceCustomer(item)}
                    >
                      <Eye className="h-4 w-4" />
                    </Button>
                    <Tooltip.Content>查看详情</Tooltip.Content>
                  </Tooltip>
                  <IconAction
                    label={item.status === "active" ? "停用" : "启用"}
                    onPress={() => void toggle(item)}
                  >
                    {item.status === "active" ? (
                      <CircleStop />
                    ) : (
                      <CirclePlayFill />
                    )}
                  </IconAction>
                  <Dropdown>
                    <Button
                      isIconOnly
                      size="sm"
                      variant="ghost"
                      aria-label={`${item.name}的更多操作`}
                    >
                      <Ellipsis className="h-4 w-4" />
                    </Button>
                    <Dropdown.Popover placement="bottom end">
                      <Dropdown.Menu aria-label={`${item.name}的更多操作`}>
                        <Dropdown.Item
                          id="delete-customer"
                          className="text-danger"
                          onAction={() => setDeleting(item)}
                        >
                          删除客户
                        </Dropdown.Item>
                      </Dropdown.Menu>
                    </Dropdown.Popover>
                  </Dropdown>
                </div>
              </Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
      {instanceCustomer ? (
        <CustomerBusinessModal
          customer={
            customers.find(
              (item) => item.customerId === instanceCustomer.customerId,
            ) ?? instanceCustomer
          }
          initialTab="overview"
          onEdit={() =>
            setSelected(
              customers.find(
                (item) => item.customerId === instanceCustomer.customerId,
              ) ?? instanceCustomer,
            )
          }
          onClose={() => setInstanceCustomer(null)}
        />
      ) : null}
      {selected !== undefined ? (
        <CustomerModal
          customer={selected}
          customers={customers}
          onClose={() => setSelected(undefined)}
          onSaved={refresh}
        />
      ) : null}
      {deleting ? (
        <DeleteCustomerDialog
          customer={deleting}
          onClose={() => setDeleting(null)}
          onDeleted={refresh}
        />
      ) : null}
    </>
  );
}
