"use client";
import { useState } from "react";
import {
  Alert,
  Button,
  Description,
  FieldError,
  Fieldset,
  Form,
  Input,
  Label,
  Modal,
  TextArea,
  TextField,
} from "@heroui/react";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import type { ApplicationProduct } from "../../../lib/operation-types";

export default function ApplicationGiftModal({
  product,
  onClose,
  onGranted,
}: {
  product: ApplicationProduct;
  onClose: () => void;
  onGranted: () => void;
}) {
  const [subject, setSubject] = useState("");
  const [reason, setReason] = useState("");
  const mutation = useOperationMutation();
  async function grant() {
    if (!subject.trim() || !reason.trim() || mutation.pending) return;
    await mutation.run(
      () =>
        operationCommand(
          `/api/application-products/${encodeURIComponent(product.productId)}/entitlements`,
          { subject: subject.trim(), reason: reason.trim() },
        ),
      () => {
        onGranted();
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
        <Modal.Container
          placement="center"
          scroll="inside"
          className="w-full sm:w-full"
        >
          <Modal.Dialog className="max-w-[640px]">
            <Modal.Header>
              <Modal.Heading>赠送 {product.name}</Modal.Heading>
              <Modal.CloseTrigger
                aria-label="关闭"
                isDisabled={mutation.pending}
              />
            </Modal.Header>
            <Modal.Body>
              <Form
                id="application-gift-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  void grant();
                }}
              >
                <Fieldset disabled={mutation.pending}>
                  <Fieldset.Group>
                    <Alert status="accent">
                      <Alert.Indicator />
                      <Alert.Content>
                        <Alert.Title>赠送整个商品</Alert.Title>
                        <Alert.Description>
                          向指定许可证主体发放整包权益，不替代客户购买；赠送原因与操作人将记录在审计中。
                        </Alert.Description>
                      </Alert.Content>
                    </Alert>
                    <TextField
                      isRequired
                      isDisabled={mutation.pending}
                      value={subject}
                      onChange={setSubject}
                    >
                      <Label>许可证主体</Label>
                      <Input placeholder="填写接收赠送的许可证主体" />
                      <Description>
                        请确认接收方的许可证主体，避免赠送给错误客户。
                      </Description>
                      <FieldError />
                    </TextField>
                    <TextField
                      isRequired
                      isDisabled={mutation.pending}
                      value={reason}
                      onChange={setReason}
                    >
                      <Label>赠送原因</Label>
                      <TextArea rows={3} placeholder="填写本次赠送的原因" />
                      <FieldError />
                    </TextField>
                    {mutation.error ? (
                      <Alert status="danger" role="alert">
                        <Alert.Indicator />
                        <Alert.Content>
                          <Alert.Title>{mutation.error}</Alert.Title>
                        </Alert.Content>
                      </Alert>
                    ) : null}
                  </Fieldset.Group>
                </Fieldset>
              </Form>
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
                form="application-gift-form"
                type="submit"
                isDisabled={!subject.trim() || !reason.trim()}
                isPending={mutation.pending}
              >
                确认赠送
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
