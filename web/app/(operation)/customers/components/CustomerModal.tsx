"use client";
import { Button } from "@heroui/react";
import { useState } from "react";
import { ModalShell } from "../../../components/ui/ModalShell";
import { useOperationMutation } from "../../../hooks/useOperationMutation";
import { operationCommand } from "../../../lib/operation-api";
import { CustomerFields } from "./CustomerFields";
import type { Customer } from "../../../lib/operation-types";
export default function CustomerModal({
  customer,
  customers,
  onClose,
  onSaved,
}: {
  customer: Customer | null;
  customers: Customer[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const [form, setForm] = useState({
    name: customer?.name || "",
    contactName: customer?.contactName || "",
    contactPhone: customer?.contactPhone || "",
    contactEmail: customer?.contactEmail || "",
    notes: customer?.notes || "",
  });
  const mutation = useOperationMutation();
  const [validationError, setValidationError] = useState("");
  async function save() {
    if (!form.name.trim()) {
      setValidationError("请输入客户名称。");
      return;
    }
    if (!customer && !form.contactPhone.trim()) {
      setValidationError("请输入联系电话（手机号）。");
      return;
    }
    if (
      !customer &&
      customers.some((item) => item.name.trim() === form.name.trim())
    ) {
      setValidationError("客户名称已存在，请勿重复创建。");
      return;
    }
    if (
      !customer &&
      customers.some(
        (item) => item.contactPhone.trim() === form.contactPhone.trim(),
      )
    ) {
      setValidationError("联系电话（手机号）已存在，请勿重复创建。");
      return;
    }
    setValidationError("");
    await mutation.run(
      () =>
        operationCommand(
          customer
            ? `/api/customers/${encodeURIComponent(customer.customerId)}`
            : "/api/customers",
          {
            ...form,
            name: form.name.trim(),
            contactPhone: form.contactPhone.trim(),
          },
        ),
      () => {
        onClose();
        onSaved();
      },
    );
  }
  return (
    <ModalShell
      title={customer ? "编辑客户" : "新增客户"}
      width={720}
      onClose={onClose}
    >
      <form
        className="space-y-5"
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <p className="text-sm text-muted">
          填写客户基本资料和联系方式，带 * 的为必填项。
        </p>
        <CustomerFields form={form} onChange={setForm} isNew={!customer} />
        {validationError || mutation.error ? (
          <p role="alert" className="text-sm text-danger">
            {validationError || mutation.error}
          </p>
        ) : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onPress={onClose}>
            取消
          </Button>
          <Button type="submit" isPending={mutation.pending}>
            保存客户
          </Button>
        </div>
      </form>
    </ModalShell>
  );
}
