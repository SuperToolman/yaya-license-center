"use client";
import { Input, Label, TextArea } from "@heroui/react";
import { useId } from "react";

export type CustomerFieldsValue = {
  name: string;
  contactName: string;
  contactPhone: string;
  contactEmail: string;
  notes: string;
};

export function CustomerFields({
  form,
  onChange,
  readOnly = false,
  isNew = false,
}: {
  form: CustomerFieldsValue;
  onChange?: (value: CustomerFieldsValue) => void;
  readOnly?: boolean;
  isNew?: boolean;
}) {
  const formId = useId();
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <div className="min-w-0 space-y-2">
        <Label htmlFor={`${formId}-name`}>
          客户名称{" "}
          <span aria-hidden="true" className="text-danger">
            *
          </span>
        </Label>
        <Input
          id={`${formId}-name`}
          name="name"
          className="w-full"
          placeholder={readOnly ? "未填写" : "请输入客户或企业名称"}
          readOnly={readOnly}
          value={form.name}
          onChange={(event) =>
            onChange?.({ ...form, name: event.currentTarget.value })
          }
          required={!readOnly}
        />
      </div>
      <div className="min-w-0 space-y-2">
        <Label htmlFor={`${formId}-contactName`}>联系人</Label>
        <Input
          id={`${formId}-contactName`}
          name="contactName"
          autoComplete="name"
          className="w-full"
          placeholder={readOnly ? "未填写" : "请输入联系人姓名"}
          readOnly={readOnly}
          value={form.contactName}
          onChange={(event) =>
            onChange?.({ ...form, contactName: event.currentTarget.value })
          }
        />
      </div>
      <div className="min-w-0 space-y-2">
        <Label htmlFor={`${formId}-contactPhone`}>
          联系电话{" "}
          {isNew ? (
            <span aria-hidden="true" className="text-danger">
              *
            </span>
          ) : null}
        </Label>
        <Input
          id={`${formId}-contactPhone`}
          name="contactPhone"
          type="tel"
          required={isNew && !readOnly}
          autoComplete="tel"
          className="w-full"
          placeholder={readOnly ? "未填写" : "请输入手机号或固定电话"}
          readOnly={readOnly}
          value={form.contactPhone}
          onChange={(event) =>
            onChange?.({ ...form, contactPhone: event.currentTarget.value })
          }
        />
      </div>
      <div className="min-w-0 space-y-2">
        <Label htmlFor={`${formId}-contactEmail`}>联系邮箱</Label>
        <Input
          id={`${formId}-contactEmail`}
          name="contactEmail"
          type="email"
          autoComplete="email"
          className="w-full"
          placeholder={readOnly ? "未填写" : "请输入联系邮箱"}
          readOnly={readOnly}
          value={form.contactEmail}
          onChange={(event) =>
            onChange?.({ ...form, contactEmail: event.currentTarget.value })
          }
        />
      </div>
      <div className="min-w-0 space-y-2 sm:col-span-2">
        <Label htmlFor={`${formId}-notes`}>客户备注</Label>
        <TextArea
          id={`${formId}-notes`}
          name="notes"
          rows={4}
          className="min-h-28 w-full resize-y"
          placeholder={
            readOnly ? "未填写" : "可填写客户背景、合作需求或其他补充信息"
          }
          readOnly={readOnly}
          value={form.notes}
          onChange={(event) =>
            onChange?.({ ...form, notes: event.currentTarget.value })
          }
        />
      </div>
    </div>
  );
}
