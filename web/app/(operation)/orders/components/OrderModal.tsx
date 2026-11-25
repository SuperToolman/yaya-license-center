"use client";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";

import { TrashBin } from "@gravity-ui/icons";
import {
  Button,
  Input,
  Label,
  ListBox,
  Select,
  Table,
  TextArea,
} from "@heroui/react";
import { useState } from "react";
import { ModalShell } from "../../../components/ui/ModalShell";
import { formatMoney } from "../../../lib/format";
import type {
  AiEmployeeProduct,
  Customer,
  Order,
  OrderItem,
  Provider,
} from "../../../lib/operation-types";

type ProductOption = {
  kind: OrderItem["kind"];
  referenceId?: string;
  title: string;
  priceCents: number;
  entitlementDays: number;
};
type Draft = ProductOption;

const builtInProducts: ProductOption[] = [
  {
    kind: "service",
    referenceId: "implementation",
    title: "部署实施服务",
    priceCents: 0,
    entitlementDays: 1,
  },
  {
    kind: "platform",
    referenceId: "platform",
    title: "低代码平台",
    priceCents: 0,
    entitlementDays: 365,
  },
  {
    kind: "module",
    referenceId: "communication",
    title: "通讯模型",
    priceCents: 0,
    entitlementDays: 365,
  },
];

function dateValue(timestamp?: number) {
  if (!timestamp) return "";
  const value = new Date(timestamp * 1000);
  return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
}

function productOptions(employees: AiEmployeeProduct[]) {
  return [
    ...builtInProducts,
    ...employees
      .filter((employee) => employee.status === "active")
      .map((employee) => ({
        kind: "ai_employee" as const,
        referenceId: employee.id,
        title: employee.title,
        priceCents: employee.priceCents,
        entitlementDays:
          employee.billingCycle === "month"
            ? 30
            : employee.billingCycle === "year"
              ? 365
              : 365,
      })),
  ];
}

export default function OrderModal({
  order,
  customerId: presetCustomerId,
  customers,
  employees,
  providers,
  onClose,
  onSaved,
}: {
  order: Order | null;
  customerId?: string;
  customers: Customer[];
  employees: AiEmployeeProduct[];
  providers: Provider[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const options = productOptions(employees);
  const [customerId, setCustomerId] = useState(
    order?.customerId || presetCustomerId || "",
  );
  const [providerId, setProviderId] = useState(order?.providerId || "");
  const [deploymentType, setDeploymentType] = useState<Order["deploymentType"]>(
    order?.deploymentType || "saas",
  );
  const [dueDate, setDueDate] = useState(dateValue(order?.dueAt));
  const [notes, setNotes] = useState(order?.notes || "");
  const [items, setItems] = useState<Draft[]>(
    order?.items.map((item) => ({
      kind: item.kind,
      referenceId: item.referenceId,
      title: item.title,
      priceCents: item.unitPriceCents,
      entitlementDays: item.entitlementDays || 365,
    })) || [builtInProducts[0]],
  );
  const [error, setError] = useState("");
  const mutation = useOperationMutation();
  const saving = mutation.pending;
  const total = items.reduce((sum, item) => sum + item.priceCents, 0);

  function addProduct() {
    const next = options.find(
      (option) =>
        !items.some(
          (item) =>
            item.kind === option.kind &&
            item.referenceId === option.referenceId,
        ),
    );
    if (next) setItems((current) => [...current, next]);
  }

  function changeProduct(index: number, key: string) {
    const option = options.find(
      (item) => `${item.kind}:${item.referenceId || item.title}` === key,
    );
    if (option)
      setItems((current) =>
        current.map((item, itemIndex) => (itemIndex === index ? option : item)),
      );
  }

  async function save() {
    setError("");
    if (!customerId) return setError("请选择客户。");

    if (items.length === 0) return setError("请添加本次购买的商品。");
    if (items.some((item) => item.priceCents < 0 || item.entitlementDays < 1))
      return setError("商品配置无效，请检查商品目录。");
    setError("");
    await mutation.run(
      async () => {
        const body = {
          customerId,
          items: items.map(
            ({ kind, referenceId, title, priceCents, entitlementDays }) => ({
              kind,
              referenceId,
              title,
              quantity: 1,
              unitPriceCents: priceCents,
              entitlementDays,
            }),
          ),
          deploymentType,
          dueAt: dueDate ? new Date(`${dueDate}T23:59:59`).toISOString() : null,
          notes,
          ...(!order && providerId ? { providerId } : {}),
        };
        await operationCommand(
          order ? `/api/orders/${order.orderId}` : "/api/orders",
          body,
          order ? "PUT" : "POST",
        );
      },
      () => {
        onClose();
        onSaved();
      },
    );
  }

  const fieldClass = "w-full";
  const selectedKey = (item: Draft) =>
    `${item.kind}:${item.referenceId || item.title}`;
  return (
    <ModalShell
      title={order ? "编辑订单" : "新增订单"}
      width={1120}
      onClose={onClose}
    >
      <p className="mb-4 text-sm text-muted">
        仅填写本次购买的商品和天数。重复购买同一权益会顺延期限；首次授权须包含平台，增购会保留已有权益，授权商品数量须为
        1。
      </p>
      <form
        className="space-y-5"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <div className="grid gap-4 md:grid-cols-3">
          <div>
            <Label>客户</Label>
            <Select
              aria-label="客户"
              isDisabled={!!presetCustomerId}
              selectedKey={customerId}
              onSelectionChange={(key) => setCustomerId(String(key))}
              className={fieldClass}
            >
              <Select.Trigger>
                <Select.Value />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  {customers
                    .filter(
                      (customer) =>
                        customer.status === "active" ||
                        customer.customerId === order?.customerId,
                    )
                    .map((customer) => (
                      <ListBox.Item
                        key={customer.customerId}
                        id={customer.customerId}
                      >
                        {customer.name}
                      </ListBox.Item>
                    ))}
                </ListBox>
              </Select.Popover>
            </Select>
          </div>
          <div>
            <Label>预计回款日期</Label>
            <Input
              aria-label="预计回款日期"
              type="date"
              value={dueDate}
              onChange={(event) => setDueDate(event.currentTarget.value)}
              className={fieldClass}
            />
          </div>
          <div>
            <Label>平台类型</Label>
            <Select
              aria-label="平台类型"
              selectedKey={deploymentType}
              onSelectionChange={(key) =>
                setDeploymentType(String(key) as Order["deploymentType"])
              }
              className={fieldClass}
            >
              <Select.Trigger>
                <Select.Value />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  <ListBox.Item id="saas">SaaS</ListBox.Item>
                  <ListBox.Item id="local">本地部署</ListBox.Item>
                </ListBox>
              </Select.Popover>
            </Select>
          </div>
        </div>
        {!order ? (
          <div className="max-w-md">
            <Label>接单服务商</Label>
            <Select
              aria-label="接单服务商"
              selectedKey={providerId}
              onSelectionChange={(key) => setProviderId(String(key))}
              className={fieldClass}
            >
              <Select.Trigger>
                <Select.Value />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  <ListBox.Item id="">平台直营</ListBox.Item>
                  {providers
                    .filter((provider) => provider.status === "active")
                    .map((provider) => (
                      <ListBox.Item
                        key={provider.providerId}
                        id={provider.providerId}
                      >
                        {provider.name}
                      </ListBox.Item>
                    ))}
                </ListBox>
              </Select.Popover>
            </Select>
          </div>
        ) : null}
        <section>
          <div className="mb-2 flex items-center justify-between">
            <div>
              <h3 className="text-sm font-semibold">订单商品</h3>
              <p className="text-xs text-muted">商品决定后续许可证授权内容。</p>
            </div>
            <Button
              type="button"
              size="sm"
              onPress={addProduct}
              isDisabled={items.length >= options.length}
            >
              ＋ 添加商品
            </Button>
          </div>
          <Table>
            <Table.ScrollContainer>
              <Table.Content aria-label="订单商品" className="min-w-[640px]">
                <Table.Header>
                  <Table.Column id="product" isRowHeader>
                    商品
                  </Table.Column>
                  <Table.Column id="kind">类型</Table.Column>
                  <Table.Column id="price">单价（元）</Table.Column>
                  <Table.Column id="days">授权天数</Table.Column>
                  <Table.Column id="actions">操作</Table.Column>
                </Table.Header>
                <Table.Body renderEmptyState={() => "暂无商品"}>
                  {items.map((item, index) => (
                    <Table.Row
                      key={`${selectedKey(item)}-${index}`}
                      id={`${selectedKey(item)}-${index}`}
                    >
                      <Table.Cell>
                        <Select
                          aria-label={`第 ${index + 1} 个商品`}
                          selectedKey={selectedKey(item)}
                          onSelectionChange={(key) =>
                            changeProduct(index, String(key))
                          }
                          className={fieldClass}
                        >
                          <Select.Trigger>
                            <Select.Value />
                          </Select.Trigger>
                          <Select.Popover>
                            <ListBox>
                              {options.map((option) => (
                                <ListBox.Item
                                  key={selectedKey(option)}
                                  id={selectedKey(option)}
                                >
                                  {option.title}
                                </ListBox.Item>
                              ))}
                            </ListBox>
                          </Select.Popover>
                        </Select>
                      </Table.Cell>
                      <Table.Cell>
                        {item.kind === "platform"
                          ? "平台模块"
                          : item.kind === "module"
                            ? "模块"
                            : "AI 员工"}
                      </Table.Cell>
                      <Table.Cell>
                        <Input
                          aria-label="商品单价"
                          type="number"
                          min={0}
                          step="0.01"
                          required
                          value={String(item.priceCents / 100)}
                          onChange={(e) =>
                            setItems((old) =>
                              old.map((row, i) =>
                                i === index
                                  ? {
                                      ...row,
                                      priceCents: Math.round(
                                        Number(e.target.value) * 100,
                                      ),
                                    }
                                  : row,
                              ),
                            )
                          }
                        />
                      </Table.Cell>
                      <Table.Cell>
                        <Input
                          aria-label="商品授权天数"
                          type="number"
                          min={1}
                          max={36500}
                          required
                          value={String(item.entitlementDays)}
                          onChange={(e) =>
                            setItems((old) =>
                              old.map((row, i) =>
                                i === index
                                  ? {
                                      ...row,
                                      entitlementDays: Number(e.target.value),
                                    }
                                  : row,
                              ),
                            )
                          }
                        />
                      </Table.Cell>
                      <Table.Cell>
                        <Button
                          type="button"
                          isIconOnly
                          aria-label="删除商品"
                          onPress={() =>
                            setItems((current) =>
                              current.filter(
                                (_, itemIndex) => itemIndex !== index,
                              ),
                            )
                          }
                          isDisabled={items.length === 1}
                        >
                          <TrashBin />
                        </Button>
                      </Table.Cell>
                    </Table.Row>
                  ))}
                </Table.Body>
              </Table.Content>
            </Table.ScrollContainer>
          </Table>
          <div className="flex justify-between px-1 py-3">
            <span>共 {items.length} 项商品</span>
            <strong>订单合计 {formatMoney(total)}</strong>
          </div>
        </section>
        <div>
          <Label>订单备注</Label>
          <TextArea
            aria-label="订单备注"
            value={notes}
            onChange={(event) => setNotes(event.currentTarget.value)}
            className={fieldClass}
          />
        </div>
        {error || mutation.error ? (
          <p role="alert" className="text-sm text-danger">
            {error || mutation.error}
          </p>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button type="button" onPress={onClose}>
            取消
          </Button>
          <Button type="submit" isPending={saving}>
            {order ? "保存订单" : "创建订单"}
          </Button>
        </div>
      </form>
    </ModalShell>
  );
}
