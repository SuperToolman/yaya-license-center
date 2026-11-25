"use client";
import { Table } from "@heroui/react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { DataPanel } from "../../../components/ui/DataPanel";
import { DataTable } from "../../../components/ui/DataTable";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { formatDate, formatMoney } from "../../../lib/format";
import type {
  CommissionEntry,
  CommissionRule,
} from "../../../lib/operation-types";

export default function CommissionsPage() {
  const rules = useData<CommissionRule[]>("/api/commission-rules");
  const entries = useData<CommissionEntry[]>("/api/commissions");
  return (
    <>
      <OperationPageHeader
        eyebrow="COMMISSION MANAGEMENT"
        title="提成与返利"
        description="按产品类型配置返点并核对回款返利。"
        loading={rules.loading || entries.loading}
        onRefresh={rules.refresh}
      />
      {rules.error || entries.error ? (
        <p role="alert" className="mb-4 text-sm text-danger">
          {rules.error || entries.error}
        </p>
      ) : null}
      <DataPanel title="产品提成规则" note="新规则只影响其生效后的回款。">
        <DataTable
          label="提成规则"
          empty="尚未配置提成规则。"
          columns={["产品类型", "返点", "生效时间", "状态"]}
        >
          {(rules.data || []).map((item) => (
            <Table.Row key={item.ruleId} id={item.ruleId}>
              <Table.Cell>{item.productType}</Table.Cell>
              <Table.Cell>
                {(item.rateBasisPoints / 100).toFixed(2)}%
              </Table.Cell>
              <Table.Cell>{formatDate(item.effectiveAt)}</Table.Cell>
              <Table.Cell>
                <StatusChip status={item.status} />
              </Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
      <DataPanel title="返利台账" note="回款时生成。">
        <DataTable
          label="返利台账"
          empty="暂无回款返利。"
          columns={["订单", "服务商", "产品", "回款", "返点", "应返", "状态"]}
        >
          {(entries.data || []).map((item) => (
            <Table.Row key={item.entryId} id={item.entryId}>
              <Table.Cell>{item.orderId}</Table.Cell>
              <Table.Cell>{item.providerId}</Table.Cell>
              <Table.Cell>{item.productType}</Table.Cell>
              <Table.Cell>{formatMoney(item.receivedAmountCents)}</Table.Cell>
              <Table.Cell>
                {(item.rateBasisPoints / 100).toFixed(2)}%
              </Table.Cell>
              <Table.Cell>{formatMoney(item.commissionAmountCents)}</Table.Cell>
              <Table.Cell>
                <StatusChip status={item.status} />
              </Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
    </>
  );
}
