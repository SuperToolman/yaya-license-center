"use client";

import { TextArea } from "@heroui/react";
import Link from "next/link";
import { ModalShell } from "../../../components/ui/ModalShell";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { formatDate } from "../../../lib/format";
import type { License, Order } from "../../../lib/operation-types";

export default function LicenseDetailModal({
  license,
  order,
  onClose,
}: {
  license: License;
  order?: Order;
  onClose: () => void;
}) {
  const legacy = license.linkageStatus === "legacy_unlinked";
  const fields = [
    ["客户", license.customerNameSnapshot || order?.customerName || "未记录"],
    ["授权主体", license.subject],
    ["许可证 ID", license.licenseId],
    ["签发时间", formatDate(license.issuedAt)],
    ["平台有效至", formatDate(license.expiresAt)],
    [
      "部署方式",
      order
        ? order.deploymentType === "local"
          ? "本地部署"
          : "SaaS"
        : "未记录",
    ],
  ];
  return (
    <ModalShell title="许可证详情" width={1040} onClose={onClose}>
      <div className="space-y-6">
        <section aria-label="许可证基础信息">
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold">基础信息</h3>
            <StatusChip status={license.platformStatus} />
            <StatusChip
              status={legacy ? "legacy_unlinked" : "linked"}
              label={legacy ? "历史未关联" : "已关联订单"}
            />
          </div>
          <dl className="grid gap-4 rounded-xl bg-surface p-4 text-sm sm:grid-cols-2">
            {fields.map(([label, value]) => (
              <div key={label} className="min-w-0">
                <dt className="mb-1 text-xs text-muted">{label}</dt>
                <dd className="break-all">{value || "未记录"}</dd>
              </div>
            ))}
            <div className="min-w-0 sm:col-span-2">
              <dt className="mb-1 text-xs text-muted">关联订单</dt>
              <dd className="break-all">
                {order ? (
                  <Link href="/orders" className="text-accent">
                    {order.orderNo}
                  </Link>
                ) : legacy ? (
                  "历史无订单"
                ) : license.orderId ? (
                  `订单已不存在（${license.orderId}）`
                ) : (
                  "未记录"
                )}
              </dd>
            </div>
          </dl>
        </section>
        <section aria-label="模块授权">
          <h3 className="mb-3 text-sm font-semibold">
            模块授权（{license.modules.length}）
          </h3>
          <div className="space-y-2">
            {license.modules.map((id) => (
              <div
                key={id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border p-3 text-sm"
              >
                <div className="min-w-0 flex-1 basis-40">
                  <p className="break-words font-medium">
                    {license.moduleTitles[id] ||
                      (
                        {
                          platform: "低代码平台",
                          communication: "通讯模型",
                        } as Record<string, string>
                      )[id] ||
                      id}
                  </p>
                  <p className="break-all font-mono text-xs text-muted">{id}</p>
                </div>
                <p>有效至：{formatDate(license.moduleExpiresAt[id])}</p>
                {license.moduleStatuses[id] ? (
                  <StatusChip status={license.moduleStatuses[id]} />
                ) : (
                  <span className="text-muted">状态未记录</span>
                )}
              </div>
            ))}
            {!license.modules.length ? (
              <p className="text-sm text-muted">无模块授权</p>
            ) : null}
          </div>
        </section>
        <section aria-label="AI 员工授权">
          <h3 className="mb-3 text-sm font-semibold">
            AI 员工授权（{license.aiEmployees.length}）
          </h3>
          <div className="space-y-2">
            {license.aiEmployees.map((employee) => (
              <div
                key={employee.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border p-3 text-sm"
              >
                <div className="min-w-0 flex-1 basis-40">
                  <p className="break-words font-medium">{employee.title}</p>
                  <p className="break-all font-mono text-xs text-muted">
                    {employee.id}
                  </p>
                </div>
                <p>有效至：{formatDate(employee.expiresAt)}</p>
                {license.aiEmployeeStatuses[employee.id] ? (
                  <StatusChip
                    status={license.aiEmployeeStatuses[employee.id]}
                  />
                ) : (
                  <span className="text-muted">状态未记录</span>
                )}
              </div>
            ))}
            {!license.aiEmployees.length ? (
              <p className="text-sm text-muted">无 AI 员工授权</p>
            ) : null}
          </div>
        </section>
        <details className="rounded-xl border border-border p-4">
          <summary className="cursor-pointer text-sm font-semibold">
            完整许可证
          </summary>
          <TextArea
            aria-label="完整许可证"
            className="mt-3 h-48 w-full resize-y break-all font-mono text-xs"
            value={license.license}
            readOnly
          />
        </details>
      </div>
    </ModalShell>
  );
}
