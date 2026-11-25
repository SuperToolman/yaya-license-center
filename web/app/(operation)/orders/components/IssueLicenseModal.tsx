"use client";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";

import { Button, Label, Modal } from "@heroui/react";
import { useState } from "react";
import type { Order } from "../../../lib/operation-types";

export default function IssueLicenseModal({
  order,
  onClose,
  onSaved,
}: {
  order: Order;
  onClose: () => void;
  onSaved: () => void;
}) {
  const mutation = useOperationMutation();
  const saving = mutation.pending;
  const [error, setError] = useState("");

  async function issue() {
    setError("");
    await mutation.run(
      async () => {
        await operationCommand("/api/licenses", {
          orderId: order.orderId,
        });
      },
      () => {
        onClose();
        onSaved();
      },
    );
  }

  return (
    <Modal isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Backdrop isDismissable>
        <Modal.Container placement="center" scroll="inside" size="md">
          <Modal.Dialog>
            <Modal.Header>
              <div>
                <p className="text-xs font-semibold tracking-wide text-accent">
                  {order.orderNo}
                </p>
                <Modal.Heading>交付客户授权</Modal.Heading>
              </div>
              <Modal.CloseTrigger aria-label="关闭" />
            </Modal.Header>
            <Modal.Body className="p-5">
              <div className="grid grid-cols-3 gap-4 text-sm">
                <div>
                  <Label>客户</Label>
                  <p className="mt-1">{order.customerName}</p>
                </div>
                <div>
                  <Label>订单</Label>
                  <p className="mt-1">{order.orderNo}</p>
                </div>
                <div>
                  <Label>状态</Label>
                  <p className="mt-1">已回款</p>
                </div>
              </div>
              <p className="mt-6 text-sm text-muted">
                按本次订单累计客户权益：保留已有模块与 AI
                员工，续费从原到期日与当前时间中的较晚者顺延。交付后生成新的许可证版本，客户仍使用同一份授权；已有实例需同步授权，不会重建容器。
              </p>
              {error || mutation.error ? (
                <p role="alert" className="mt-4 text-sm text-danger">
                  {error || mutation.error}
                </p>
              ) : null}
              <div className="mt-6 flex justify-end gap-2">
                <Button type="button" variant="ghost" onPress={onClose}>
                  取消
                </Button>
                <Button
                  type="button"
                  isPending={saving}
                  onPress={() => void issue()}
                >
                  确认交付
                </Button>
              </div>
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
