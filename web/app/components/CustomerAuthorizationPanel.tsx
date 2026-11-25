"use client";
import { Eye } from "@gravity-ui/icons";
import { Table } from "@heroui/react";
import { useState } from "react";
import { useOperation } from "./OperationProvider";
import { operationCommand, operationRequest } from "../lib/operation-api";
import type { CustomerAuthorization } from "../lib/types/licenses";
import type { License } from "../lib/operation-types";
import { formatDate } from "../lib/format";
import { DataPanel } from "./ui/DataPanel";
import { DataTable } from "./ui/DataTable";
import { ConfirmAction } from "./ui/ConfirmAction";
import { IconAction } from "./ui/IconAction";
import { SemanticChip, type ChipColor } from "./ui/SemanticChip";
import LicenseDetailModal from "../(operation)/licenses/components/LicenseDetailModal";
import { useEffect } from "react";
export const authorizationLabels = {
  pending: "待应用",
  active: "当前生效",
  superseded: "已被替代",
  revoked: "已撤销",
};
const authorizationColors = {
  pending: "warning",
  active: "success",
  superseded: "default",
  revoked: "danger",
} satisfies Record<keyof typeof authorizationLabels, ChipColor>;
export function useCustomerAuthorization(customerId: string) {
  const [value, setValue] = useState<CustomerAuthorization | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const { refreshToken: stamp } = useOperation();
  useEffect(() => {
    const controller = new AbortController();
    void operationRequest<CustomerAuthorization | null>(
      `/api/customers/${encodeURIComponent(customerId)}/authorization`,
      { signal: controller.signal },
    )
      .then(({ response, payload }) => {
        if (controller.signal.aborted) return;
        if (!response.ok) throw Error(payload.message);
        setValue(payload.data ?? null);
        setError("");
      })
      .catch((e) => {
        if (!controller.signal.aborted)
          setError(e instanceof Error ? e.message : "授权读取失败");
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [customerId, stamp]);
  return { data: value, error, loading };
}
export default function CustomerAuthorizationPanel({
  customerId,
}: {
  customerId: string;
}) {
  const query = useCustomerAuthorization(customerId);
  const { currentUser, refresh } = useOperation();
  const [selected, setSelected] = useState<License | null>(null);
  const auth = query.data;
  if (query.loading) return <p>正在读取客户授权…</p>;
  if (query.error)
    return (
      <p role="alert" className="text-danger">
        {query.error}
      </p>
    );
  if (!auth)
    return (
      <p className="text-sm text-muted">
        客户尚无平台授权，请在订单中完成首次购买、回款和授权交付。
      </p>
    );
  const latest = auth.latest;
  const effective = auth.versions.find(
    (v) => v.license.licenseId === auth.activeLicenseId,
  );
  return (
    <div className="space-y-4">
      <DataPanel title="客户授权" note={`授权标识：${auth.authorizationId}`}>
        <div className="flex flex-wrap items-center justify-between gap-3 px-4 pt-4">
          <div>
            <h3 className="text-sm font-semibold">客户授权</h3>
            <p className="mt-1 break-all text-xs text-muted">
              授权标识：{auth.authorizationId}
            </p>
          </div>
          {latest &&
          latest.platformStatus !== "destroyed" &&
          latest.linkageStatus !== "legacy_unlinked" &&
          currentUser?.role === "platform_admin" ? (
            <ConfirmAction
              label="撤销客户授权"
              description="确认撤销该客户的全部授权吗？所有许可证版本将被标记为已撤销，并清除当前生效版本。"
              danger
              onConfirm={async () => {
                await operationCommand(
                  `/api/licenses/${encodeURIComponent(latest.licenseId)}/destroy`,
                );
                refresh();
                return true;
              }}
            />
          ) : null}
        </div>
        <dl className="grid gap-3 p-4 text-sm sm:grid-cols-2">
          <div>
            <dt className="text-muted">最新签发版本</dt>
            <dd>
              第{" "}
              {auth.versions.find(
                (v) => v.license.licenseId === auth.latestLicenseId,
              )?.revision ?? auth.revision}{" "}
              版 ·{" "}
              {latest?.platformStatus === "destroyed"
                ? "已撤销"
                : latest?.platformStatus === "expired"
                  ? "已过期"
                  : auth.latestLicenseId === auth.activeLicenseId
                    ? "已应用"
                    : "待应用"}
            </dd>
          </div>
          <div>
            <dt className="text-muted">当前生效版本</dt>
            <dd>{effective ? `第 ${effective.revision} 版` : "尚未激活"}</dd>
          </div>
          <div>
            <dt className="text-muted">平台有效至</dt>
            <dd>{latest ? formatDate(latest.expiresAt) : "无可用许可证"}</dd>
          </div>
          <div>
            <dt className="text-muted">授权范围</dt>
            <dd>
              {latest?.modules
                .map((m) => latest.moduleTitles[m] || m)
                .join("、") || "无"}
              ；AI 员工 {latest?.aiEmployees.length ?? 0} 个
            </dd>
          </div>
        </dl>
        <p className="px-4 pb-4 text-xs text-muted">
          新订单只增加本次购买权益；旧版本保留追溯。签发成功后，可在平台实例中同步授权。
        </p>
      </DataPanel>
      <DataPanel title="许可证版本历史" note="签发快照保留用于追溯">
        <h3 className="px-4 py-3 text-sm font-semibold">许可证版本历史</h3>
        <DataTable
          label="许可证版本历史"
          columns={["版本", "来源订单", "签发时间", "状态", "操作"]}
          empty="暂无版本"
        >
          {auth.versions.map((v) => (
            <Table.Row key={v.license.licenseId}>
              <Table.Cell>第 {v.revision} 版</Table.Cell>
              <Table.Cell>{v.license.orderId}</Table.Cell>
              <Table.Cell>{formatDate(v.license.issuedAt)}</Table.Cell>
              <Table.Cell>
                <SemanticChip
                  color={authorizationColors[v.state]}
                  className={
                    v.state === "superseded" ? "bg-default" : undefined
                  }
                >
                  {authorizationLabels[v.state]}
                </SemanticChip>
              </Table.Cell>
              <Table.Cell>
                <IconAction
                  label="查看许可证"
                  onPress={() => setSelected(v.license)}
                >
                  <Eye />
                </IconAction>
              </Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
      {selected ? (
        <LicenseDetailModal
          license={selected}
          onClose={() => setSelected(null)}
        />
      ) : null}
    </div>
  );
}
