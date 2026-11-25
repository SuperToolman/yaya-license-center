"use client";

import { Card, Drawer, Label, Table, TextArea } from "@heroui/react";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { formatDate, formatMoney } from "../../../lib/format";
import type { License, Order } from "../../../lib/operation-types";

function DetailField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="min-w-0">
      <Label className="text-xs text-muted">{label}</Label>
      <div className="mt-1 break-words text-sm">{children}</div>
    </div>
  );
}

export default function OrderDetailPanel({
  order,
  license,
  onClose,
}: {
  order: Order;
  license?: License;
  onClose: () => void;
}) {
  return (
    <Drawer isOpen onOpenChange={(open) => !open && onClose()}>
      <Drawer.Backdrop isDismissable>
        <Drawer.Content placement="right">
          <Drawer.Dialog className="h-full min-h-0 w-full min-w-0 max-w-[50rem] overflow-hidden p-0">
            <Drawer.Header className="shrink-0 px-6 py-5">
              <div>
                <p className="text-xs font-semibold tracking-wide text-accent">
                  ORDER DETAIL
                </p>
                <Drawer.Heading>{order.orderNo}</Drawer.Heading>
              </div>
              <Drawer.CloseTrigger aria-label="关闭" />
            </Drawer.Header>
            <Drawer.Body className="m-0 min-h-0 min-w-0 flex-1 space-y-4 overflow-x-hidden overflow-y-auto p-5">
              <Card variant="secondary" className="p-5">
                <h2 className="mb-5 text-sm font-semibold">订单信息</h2>
                <div className="grid gap-5 sm:grid-cols-2">
                  <DetailField label="订单 ID">{order.orderId}</DetailField>
                  <DetailField label="客户">{order.customerName}</DetailField>
                  <DetailField label="订单状态">
                    <StatusChip status={order.status} />
                  </DetailField>
                  <DetailField label="部署类型">
                    {order.deploymentType === "saas" ? "SaaS" : "本地部署"}
                  </DetailField>
                  <DetailField label="创建时间">
                    {formatDate(order.createdAt)}
                  </DetailField>
                  <DetailField label="预计回款">
                    {formatDate(order.dueAt)}
                  </DetailField>
                  <DetailField label="订单金额">
                    {formatMoney(order.totalAmountCents)}
                  </DetailField>
                  <DetailField label="已到账金额">
                    {formatMoney(order.receivedAmountCents)}
                  </DetailField>
                  <div className="sm:col-span-2">
                    <DetailField label="备注">{order.notes || "-"}</DetailField>
                  </div>
                </div>
              </Card>

              <Card variant="secondary" className="p-5">
                <h2 className="mb-5 text-sm font-semibold">商品明细</h2>
                <Table>
                  <Table.ScrollContainer>
                    <Table.Content aria-label="订单商品明细">
                      <Table.Header>
                        <Table.Column id="product" isRowHeader>
                          商品
                        </Table.Column>
                        <Table.Column id="amount">金额</Table.Column>
                      </Table.Header>
                      <Table.Body renderEmptyState={() => "暂无商品明细"}>
                        {order.items.map((item, index) => (
                          <Table.Row key={index} id={String(index)}>
                            <Table.Cell>{item.title}</Table.Cell>
                            <Table.Cell>
                              {formatMoney(item.unitPriceCents)}
                            </Table.Cell>
                          </Table.Row>
                        ))}
                      </Table.Body>
                    </Table.Content>
                  </Table.ScrollContainer>
                </Table>
              </Card>

              {license ? (
                <Card variant="secondary" className="p-5">
                  <h2 className="mb-5 text-sm font-semibold">关联许可证</h2>
                  <div className="grid gap-5 sm:grid-cols-2">
                    <DetailField label="许可证 ID">
                      {license.licenseId}
                    </DetailField>
                    <DetailField label="许可证状态">
                      <StatusChip status={license.platformStatus} />
                    </DetailField>
                    <DetailField label="签发时间">
                      {formatDate(license.issuedAt)}
                    </DetailField>
                    <DetailField label="有效至">
                      {formatDate(license.expiresAt)}
                    </DetailField>
                    <DetailField label="授权主体">
                      {license.subject}
                    </DetailField>
                    <DetailField label="客户快照">
                      {license.customerNameSnapshot || order.customerName}
                    </DetailField>
                  </div>
                  <div className="mt-6">
                    <div className="mb-2 flex items-center justify-between">
                      <h3 className="text-sm font-semibold">模块授权</h3>
                      <span className="text-xs text-muted">
                        {license.modules.length} 项
                      </span>
                    </div>
                    {license.modules.map((module) => (
                      <div
                        key={module}
                        className="flex flex-wrap items-center justify-between gap-3 py-3"
                      >
                        <div className="min-w-0 flex-1 break-words">
                          <span className="block text-sm">
                            {license.moduleTitles[module] || module}
                          </span>
                          <small className="text-xs text-muted">{module}</small>
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                          <StatusChip
                            status={
                              license.moduleStatuses[module] ||
                              license.platformStatus
                            }
                          />
                          <small className="text-xs text-muted">
                            {formatDate(
                              license.moduleExpiresAt[module] ||
                                license.expiresAt,
                            )}
                          </small>
                        </div>
                      </div>
                    ))}
                  </div>
                  <div className="mt-6">
                    <div className="mb-2 flex items-center justify-between">
                      <h3 className="text-sm font-semibold">AI 员工授权</h3>
                      <span className="text-xs text-muted">
                        {license.aiEmployees.length} 项
                      </span>
                    </div>
                    {license.aiEmployees.map((employee) => (
                      <div
                        key={employee.id}
                        className="flex flex-wrap items-center justify-between gap-3 py-3"
                      >
                        <div className="min-w-0 flex-1 break-words">
                          <span className="block text-sm">
                            {employee.title}
                          </span>
                          <small className="text-xs text-muted">
                            {employee.id}
                          </small>
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                          <StatusChip
                            status={
                              license.aiEmployeeStatuses[employee.id] ||
                              license.platformStatus
                            }
                          />
                          <small className="text-xs text-muted">
                            {formatDate(employee.expiresAt)}
                          </small>
                        </div>
                      </div>
                    ))}
                  </div>
                  <div className="mt-6">
                    <Label>完整签名</Label>
                    <TextArea
                      aria-label="完整签名"
                      className="mt-2 min-h-44 w-full font-mono text-xs"
                      value={license.license}
                      readOnly
                    />
                  </div>
                </Card>
              ) : (
                <Card variant="secondary" className="p-5">
                  <h2 className="text-sm font-semibold">关联许可证</h2>
                  <p className="mt-3 text-sm text-muted">
                    该订单尚未签发许可证。
                  </p>
                </Card>
              )}
            </Drawer.Body>
          </Drawer.Dialog>
        </Drawer.Content>
      </Drawer.Backdrop>
    </Drawer>
  );
}
