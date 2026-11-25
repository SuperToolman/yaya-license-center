"use client";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";

import {
  Alert,
  Button,
  Description,
  FieldError,
  Form,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  TextArea,
  TextField,
} from "@heroui/react";
import { useId, useState } from "react";
import { formatMoney } from "../../../lib/format";
import type { Order } from "../../../lib/operation-types";

const methods = [
  { id: "bank_transfer", label: "银行转账" },
  { id: "alipay", label: "支付宝" },
  { id: "wechat", label: "微信支付" },
  { id: "cash", label: "现金" },
  { id: "other", label: "其他" },
];

function toCents(value: string) {
  if (!/^\d+(?:\.\d{1,2})?$/.test(value.trim())) return null;
  const [yuan, cents = ""] = value.trim().split(".");
  const amount = Number(yuan) * 100 + Number(cents.padEnd(2, "0"));
  return Number.isSafeInteger(amount) ? amount : null;
}

export default function PaymentModal({
  order,
  onClose,
  onSaved,
}: {
  order: Order;
  onClose: () => void;
  onSaved: () => void;
}) {
  const outstanding = Math.max(
    0,
    order.totalAmountCents - order.receivedAmountCents,
  );
  const [amount, setAmount] = useState((outstanding / 100).toFixed(2));
  const [method, setMethod] = useState("bank_transfer");
  const [reference, setReference] = useState("");
  const [notes, setNotes] = useState("");
  const [error, setError] = useState("");
  const formId = useId();
  const mutation = useOperationMutation();
  const saving = mutation.pending;

  async function save() {
    const amountCents = toCents(amount);
    if (!amountCents || amountCents <= 0 || amountCents > outstanding) {
      setError(`本次到账金额必须在 0 至 ${formatMoney(outstanding)} 之间。`);
      return;
    }
    setError("");
    await mutation.run(
      async () => {
        await operationCommand(`/api/orders/${order.orderId}/payment`, {
          amountCents,
          method,
          reference,
          notes,
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
              <Modal.Heading>登记回款</Modal.Heading>
              <Description>{order.orderNo}</Description>
              <Modal.CloseTrigger aria-label="关闭" />
            </Modal.Header>
            <Modal.Body>
              <Form
                id={formId}
                className="flex flex-col gap-4"
                onSubmit={(event) => {
                  event.preventDefault();
                  void save();
                }}
              >
                <dl className="grid grid-cols-3 gap-4 text-sm">
                  <div>
                    <dt className="text-muted">客户</dt>
                    <dd>{order.customerName}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">订单金额</dt>
                    <dd>{formatMoney(order.totalAmountCents)}</dd>
                  </div>
                  <div>
                    <dt className="text-muted">待收金额</dt>
                    <dd>{formatMoney(outstanding)}</dd>
                  </div>
                </dl>
                <TextField
                  fullWidth
                  isRequired
                  isInvalid={!!error}
                  value={amount}
                  onChange={(value) => {
                    setAmount(value);
                    setError("");
                  }}
                >
                  <Label>本次到账（元）</Label>
                  <Input
                    type="number"
                    min="0.01"
                    max={(outstanding / 100).toFixed(2)}
                    step="0.01"
                  />
                  <FieldError>{error || undefined}</FieldError>
                </TextField>
                <Select
                  fullWidth
                  selectedKey={method}
                  onSelectionChange={(key) => setMethod(String(key))}
                >
                  <Label>收款方式</Label>
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      {methods.map((item) => (
                        <ListBox.Item key={item.id} id={item.id}>
                          {item.label}
                        </ListBox.Item>
                      ))}
                    </ListBox>
                  </Select.Popover>
                </Select>
                <TextField fullWidth value={reference} onChange={setReference}>
                  <Label>银行流水 / 交易号</Label>
                  <Input />
                </TextField>
                <TextField fullWidth value={notes} onChange={setNotes}>
                  <Label>备注</Label>
                  <TextArea rows={3} />
                </TextField>
                {mutation.error ? (
                  <Alert status="danger" role="alert">
                    <Alert.Indicator />
                    <Alert.Content>
                      <Alert.Description>{mutation.error}</Alert.Description>
                    </Alert.Content>
                  </Alert>
                ) : null}
              </Form>
            </Modal.Body>
            <Modal.Footer>
              <Button type="button" variant="ghost" onPress={onClose}>
                取消
              </Button>
              <Button type="submit" form={formId} isPending={saving}>
                确认到账
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
