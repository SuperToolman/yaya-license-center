"use client";
import { Plus } from "@gravity-ui/icons";
import { Button, Input, Table } from "@heroui/react";
import { useState } from "react";
import OperationPageHeader from "../../../components/OperationPageHeader";
import { DataPanel } from "../../../components/ui/DataPanel";
import { DataTable } from "../../../components/ui/DataTable";
import { ModalShell } from "../../../components/ui/ModalShell";
import { StatusChip } from "../../../components/ui/SemanticChip";
import { useOperationData as useData } from "../../../hooks/useOperationData";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { formatDate } from "../../../lib/format";
import { operationCommand as operationJson } from "../../../lib/operation-api";
import type { Provider } from "../../../lib/operation-types";

export default function ProvidersPage() {
  const {
    data: result,
    error: queryError,
    loading,
    refresh,
  } = useData<Provider[]>("/api/providers");
  const data = result ?? [];
  const mutation = useOperationMutation();
  const error = mutation.error || queryError;
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState({
    name: "",
    contactName: "",
    contactPhone: "",
  });
  async function create() {
    await mutation.run(
      () => operationJson("/api/providers", form),
      () => {
        setOpen(false);
        refresh();
      },
    );
  }
  return (
    <>
      <OperationPageHeader
        eyebrow="PROVIDER MANAGEMENT"
        title="服务商管理"
        description="维护接单服务商及订单归属。"
        loading={loading || mutation.pending}
        onRefresh={refresh}
        actions={
          <Button isDisabled={mutation.pending} onPress={() => setOpen(true)}>
            <Plus className="h-4 w-4" />
            新增服务商
          </Button>
        }
      />
      {error ? <p className="mb-4 text-sm text-danger">{error}</p> : null}
      <DataPanel
        title="接单服务商"
        note="订单回款后将按当前有效提成规则生成返利。"
      >
        <DataTable
          label="服务商"
          empty="尚未建立服务商。"
          columns={["服务商", "联系人", "电话", "状态", "建立时间"]}
        >
          {data.map((item) => (
            <Table.Row key={item.providerId} id={item.providerId}>
              <Table.Cell>{item.name}</Table.Cell>
              <Table.Cell>{item.contactName || "-"}</Table.Cell>
              <Table.Cell>{item.contactPhone || "-"}</Table.Cell>
              <Table.Cell>
                <StatusChip status={item.status} />
              </Table.Cell>
              <Table.Cell>{formatDate(item.createdAt)}</Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
      {open ? (
        <ModalShell title="新增服务商" onClose={() => setOpen(false)}>
          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              void create();
            }}
          >
            {Object.entries(form).map(([key, value]) => (
              <Input
                key={key}
                aria-label={key}
                placeholder={key}
                value={value}
                onChange={(event) =>
                  setForm({ ...form, [key]: event.currentTarget.value })
                }
                required={key === "name"}
              />
            ))}
            {mutation.error ? <p role="alert">{mutation.error}</p> : null}
            <Button isPending={mutation.pending} type="submit">
              保存
            </Button>
          </form>
        </ModalShell>
      ) : null}
    </>
  );
}
