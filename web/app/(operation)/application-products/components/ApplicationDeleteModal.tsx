"use client";
import { Alert, Button, Modal } from "@heroui/react";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import type { ApplicationProduct } from "../../../lib/operation-types";

export default function ApplicationDeleteModal({
  product,
  onClose,
  onDeleted,
}: {
  product: ApplicationProduct;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const mutation = useOperationMutation();
  async function remove() {
    if (product.listingStatus !== "withdrawn") return;
    await mutation.run(
      () =>
        operationCommand(
          `/api/application-products/${encodeURIComponent(product.productId)}`,
          undefined,
          "DELETE",
        ),
      () => {
        onDeleted();
        onClose();
      },
    );
  }
  return (
    <Modal
      isOpen
      onOpenChange={(open) => {
        if (!open && !mutation.pending) onClose();
      }}
    >
      <Modal.Backdrop
        isDismissable={!mutation.pending}
        isKeyboardDismissDisabled={mutation.pending}
      >
        <Modal.Container placement="center" scroll="inside" size="lg">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>删除 {product.name}</Modal.Heading>
              <Modal.CloseTrigger
                aria-label="关闭"
                isDisabled={mutation.pending}
              />
            </Modal.Header>
            <Modal.Body>
              <Alert status="warning">
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title>确认删除该应用商品？</Alert.Title>
                  <Alert.Description>
                    删除后将从应用管理中移除，不能再次上架或导入新版本。历史订单、客户权益和审核记录将保留。
                  </Alert.Description>
                </Alert.Content>
              </Alert>
              {mutation.error ? (
                <Alert status="danger" role="alert">
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Title>{mutation.error}</Alert.Title>
                  </Alert.Content>
                </Alert>
              ) : null}
            </Modal.Body>
            <Modal.Footer>
              <Button
                variant="secondary"
                isDisabled={mutation.pending}
                onPress={onClose}
              >
                取消
              </Button>
              <Button
                variant="danger"
                isDisabled={product.listingStatus !== "withdrawn"}
                isPending={mutation.pending}
                onPress={() => void remove()}
              >
                确认删除
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
