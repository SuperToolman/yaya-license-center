"use client";
import { Button } from "@heroui/react";
import { AlertDialog } from "@heroui/react/alert-dialog";
import { type ReactNode, useState } from "react";
import { useOperationMutation } from "../../hooks/useOperationMutation";
import { IconAction } from "./IconAction";
export function ConfirmAction({
  label,
  children,
  onConfirm,
  danger = false,
  isIconOnly = false,
  description = "确认执行此操作吗？",
}: {
  label: string;
  children?: ReactNode;
  onConfirm: () => Promise<boolean>;
  danger?: boolean;
  isIconOnly?: boolean;
  description?: string;
}) {
  const [open, setOpen] = useState(false);
  const mutation = useOperationMutation();
  function openDialog() {
    mutation.clearError();
    setOpen(true);
  }
  return (
    <>
      {isIconOnly ? (
        <IconAction
          label={label}
          className={danger ? "text-danger" : undefined}
          onPress={openDialog}
        >
          {children}
        </IconAction>
      ) : (
        <Button
          size="sm"
          variant={danger ? "danger-soft" : "ghost"}
          onPress={openDialog}
        >
          {children || label}
        </Button>
      )}
      <AlertDialog isOpen={open} onOpenChange={setOpen}>
        <AlertDialog.Backdrop>
          <AlertDialog.Container placement="center" size="sm">
            <AlertDialog.Dialog>
              <AlertDialog.Header>
                <AlertDialog.Heading>{label}</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                {description}
                {mutation.error ? <p role="alert">{mutation.error}</p> : null}
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <Button variant="ghost" onPress={() => setOpen(false)}>
                  取消
                </Button>
                <Button
                  variant={danger ? "danger" : "primary"}
                  isPending={mutation.pending}
                  onPress={() =>
                    void mutation.run(
                      async () => {
                        if (!(await onConfirm()))
                          throw new Error("操作未完成，请检查错误信息后重试。");
                      },
                      () => setOpen(false),
                    )
                  }
                >
                  确认
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      </AlertDialog>
    </>
  );
}
