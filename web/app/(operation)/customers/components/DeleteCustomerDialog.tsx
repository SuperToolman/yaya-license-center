"use client";

import { Button } from "@heroui/react";
import { AlertDialog } from "@heroui/react/alert-dialog";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationCommand, operationData } from "../../../lib/operation-api";
import type {
  Customer,
  CustomerDeletionEligibility,
} from "../../../lib/operation-types";

export default function DeleteCustomerDialog({
  customer,
  onClose,
  onDeleted,
}: {
  customer: Customer;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const path = `/api/customers/${encodeURIComponent(customer.customerId)}`;
  const query = useOperationQuery(path, (signal) =>
    operationData<CustomerDeletionEligibility>(`${path}/deletion-eligibility`, {
      signal,
    }),
  );
  const mutation = useOperationMutation();
  const eligibility = query.loading || query.error ? null : query.data;
  const error = mutation.error || query.error;
  async function remove() {
    if (!eligibility?.canDelete || mutation.pending) return;
    const deleted = await mutation.run(
      () => operationCommand(path, undefined, "DELETE"),
      () => {
        onClose();
        onDeleted();
      },
    );
    if (!deleted) query.refresh();
  }
  return (
    <AlertDialog
      isOpen
      onOpenChange={(open) => {
        if (!open && !mutation.pending) onClose();
      }}
    >
      <AlertDialog.Backdrop>
        <AlertDialog.Container placement="center" className="w-full sm:w-full">
          <AlertDialog.Dialog style={{ width: "100%", maxWidth: 680 }}>
            <AlertDialog.Header>
              <AlertDialog.Heading>删除客户</AlertDialog.Heading>
            </AlertDialog.Header>
            <AlertDialog.Body className="max-h-[60vh] space-y-4 overflow-y-auto text-sm">
              <p className="break-all">客户：{customer.name}</p>
              {query.loading ? (
                <p role="status">正在检查客户关联数据及其下级关联数据…</p>
              ) : null}
              {error ? (
                <p role="alert" className="text-danger">
                  {error}
                </p>
              ) : null}
              {eligibility?.canDelete ? (
                <p>
                  未发现关联业务数据。确认永久删除此客户吗？此操作无法恢复。
                </p>
              ) : null}
              {eligibility && !eligibility.canDelete ? (
                <>
                  <p className="text-danger">
                    该客户仍有关联数据，暂不能删除。请管理员先手动清理以下数据及其下级关联数据，再删除客户。
                  </p>
                  <ul className="space-y-3 rounded-xl border border-border p-4">
                    {eligibility.blockers.map((item) => (
                      <li key={item.kind}>
                        <p className="font-medium">
                          {item.label}：{item.count} 条
                        </p>
                        {item.recordIds.length ? (
                          <p className="mt-1 break-all font-mono text-xs text-muted">
                            {item.recordIds.join("、")}
                            {item.count > item.recordIds.length
                              ? "（仅显示前 10 条）"
                              : ""}
                          </p>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                  <p className="text-xs text-muted">
                    取消、过期或已销毁的记录仍属于关联数据。本操作不会自动删除关联数据。清理完成后点击“重新检查”。
                  </p>
                </>
              ) : null}
            </AlertDialog.Body>
            <AlertDialog.Footer className="flex flex-wrap gap-2">
              <Button
                variant="ghost"
                isDisabled={mutation.pending}
                onPress={onClose}
              >
                取消
              </Button>
              <Button
                variant="secondary"
                isDisabled={query.loading || mutation.pending}
                onPress={() => {
                  mutation.clearError();
                  query.refresh();
                }}
              >
                重新检查
              </Button>
              <Button
                variant="danger"
                isDisabled={!eligibility?.canDelete || mutation.pending}
                isPending={mutation.pending}
                onPress={() => void remove()}
              >
                确认删除
              </Button>
            </AlertDialog.Footer>
          </AlertDialog.Dialog>
        </AlertDialog.Container>
      </AlertDialog.Backdrop>
    </AlertDialog>
  );
}
