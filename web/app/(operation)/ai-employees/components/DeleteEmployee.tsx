"use client";
import { TrashBin } from "@gravity-ui/icons";
import { Button } from "@heroui/react";
import { AlertDialog } from "@heroui/react/alert-dialog";
import { useState } from "react";
import { IconAction } from "../../../components/ui/IconAction";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { useOperationQuery } from "../../../hooks/useOperationQuery";
import { operationCommand, operationData } from "../../../lib/operation-api";
import type {
  AiEmployeeDeletionEligibility,
  AiEmployeeProduct,
} from "../../../lib/operation-types";
export default function DeleteEmployee({
  employee,
  onDeleted,
}: {
  employee: AiEmployeeProduct;
  onDeleted: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [generation, setGeneration] = useState(0);
  const mutation = useOperationMutation();
  const query = useOperationQuery(
    `${employee.id}:${open}:${generation}`,
    (signal) =>
      open
        ? operationData<AiEmployeeDeletionEligibility>(
            `/api/ai-employees/${encodeURIComponent(employee.id)}/deletion-eligibility`,
            { signal },
          )
        : Promise.resolve(null),
  );
  const eligibility = query.loading ? null : query.data;
  const checking = query.loading;
  const error = mutation.error || query.error;
  function check() {
    mutation.clearError();
    setGeneration((value) => value + 1);
    setOpen(true);
  }
  async function remove() {
    if (!eligibility?.canDelete || checking) return;
    await mutation.run(
      () =>
        operationCommand(
          `/api/ai-employees/${encodeURIComponent(employee.id)}`,
          undefined,
          "DELETE",
        ),
      () => {
        setOpen(false);
        onDeleted();
      },
    );
  }
  return (
    <>
      <IconAction
        label="删除"

        onPress={() => void check()}
      >
        <TrashBin />
      </IconAction>
      <AlertDialog isOpen={open} onOpenChange={setOpen}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container placement="center" size="sm">
            <AlertDialog.Dialog>
              <AlertDialog.Header>
                <AlertDialog.Heading>删除 AI 员工</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                {error ||
                  (eligibility?.canDelete
                    ? `确认删除“${employee.title}”吗？`
                    : eligibility
                      ? `仍有 ${eligibility.blockers.length} 个有效客户授权，无法删除。`
                      : "正在检查客户授权...")}
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <Button variant="ghost" onPress={() => setOpen(false)}>
                  取消
                </Button>
                <Button
                  variant="danger"
                  isPending={mutation.pending}
                  isDisabled={
                    !eligibility?.canDelete || checking || mutation.pending
                  }
                  onPress={() => void remove()}
                >
                  确认删除
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>
    </>
  );
}
