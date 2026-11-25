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
  OperationSkill,
  SkillDeletionEligibility,
} from "../../../lib/operation-types";
export default function DeleteSkillDialog({
  skill,
  onDeleted,
}: {
  skill: OperationSkill;
  onDeleted: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [generation, setGeneration] = useState(0);
  const mutation = useOperationMutation();
  const query = useOperationQuery(
    `${skill.id}:${open}:${generation}`,
    (signal) =>
      open
        ? operationData<SkillDeletionEligibility>(
            `/api/skills/${encodeURIComponent(skill.id)}/deletion-eligibility`,
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
          `/api/skills/${encodeURIComponent(skill.id)}`,
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
        isDisabled={mutation.pending}
        label={`删除 ${skill.title}`}

        onPress={() => void check()}
      >
        <TrashBin />
      </IconAction>
      <AlertDialog isOpen={open} onOpenChange={setOpen}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container placement="center" size="sm">
            <AlertDialog.Dialog>
              <AlertDialog.Header>
                <AlertDialog.Heading>删除 Skill</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body className="space-y-3 text-sm">
                {checking ? <p>正在检查 AI 员工引用...</p> : null}
                {error ? <p className="text-danger">{error}</p> : null}
                {eligibility?.canDelete ? (
                  <p>
                    确认永久删除“{skill.title}”及其 ZIP
                    文件包吗？此操作无法恢复。
                  </p>
                ) : null}
                {eligibility && !eligibility.canDelete ? (
                  <p>
                    该 Skill 仍被{" "}
                    {eligibility.blockers
                      .map((item) => item.employeeTitle)
                      .join("、")}{" "}
                    引用，无法删除。
                  </p>
                ) : null}
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
