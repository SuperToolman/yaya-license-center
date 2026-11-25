"use client";
import {
  Description,
  FieldError,
  Fieldset,
  InputGroup,
  Label,
  TextArea,
  TextField,
} from "@heroui/react";
import { ApplicationSelect } from "./ApplicationSelect";
export type CommercialDraft = {
  pricingMode: "free" | "paid";
  price: string;
  rate: string;
  description: string;
};
export const emptyCommercial: CommercialDraft = {
  pricingMode: "free",
  price: "",
  rate: "",
  description: "",
};
export function commercialPayload(draft: CommercialDraft) {
  const decimal = /^\d+(?:\.\d{1,2})?$/;
  if (!draft.description.trim()) throw new Error("请填写商品说明");
  if (
    draft.pricingMode === "paid" &&
    (!decimal.test(draft.price) ||
      Number(draft.price) <= 0 ||
      Number(draft.price) > 1000000 ||
      !decimal.test(draft.rate) ||
      Number(draft.rate) > 100)
  )
    throw new Error("请填写有效售价及发布者分成比例（0–100%，最多两位小数）");
  return {
    pricingMode: draft.pricingMode,
    priceCents:
      draft.pricingMode === "free" ? 0 : Math.round(Number(draft.price) * 100),
    publisherRateBasisPoints:
      draft.pricingMode === "free" ? 0 : Math.round(Number(draft.rate) * 100),
    currency: "CNY",
    licenseModel: "one_time",
    description: draft.description.trim(),
  };
}
export function CommercialFields({
  value,
  onChange,
  disabled = false,
}: {
  value: CommercialDraft;
  onChange: (value: CommercialDraft) => void;
  disabled?: boolean;
}) {
  return (
    <Fieldset disabled={disabled}>
      <Fieldset.Legend>销售与分成信息</Fieldset.Legend>
      <Fieldset.Group>
        <TextField
          isRequired
          isDisabled={disabled}
          value={value.description}
          onChange={(description) => onChange({ ...value, description })}
        >
          <Label>商品说明</Label>
          <TextArea
            maxLength={2000}
            rows={3}
            placeholder="介绍用途、整包范围及购买后可用能力"
          />
          <FieldError />
        </TextField>
        <ApplicationSelect
          label="定价方式"
          value={value.pricingMode}
          onChange={(pricingMode) =>
            onChange({
              ...value,
              pricingMode: pricingMode as CommercialDraft["pricingMode"],
            })
          }
          disabled={disabled}
          options={[
            { id: "free", label: "免费领取" },
            { id: "paid", label: "一次买断" },
          ]}
        />
        {value.pricingMode === "paid" ? (
          <>
            <TextField
              isRequired
              isDisabled={disabled}
              value={value.price}
              onChange={(price) => onChange({ ...value, price })}
            >
              <Label>整包售价</Label>
              <InputGroup>
                <InputGroup.Prefix>¥</InputGroup.Prefix>
                <InputGroup.Input
                  inputMode="decimal"
                  placeholder="例如 199.00"
                />
                <InputGroup.Suffix>元</InputGroup.Suffix>
              </InputGroup>
              <Description>一次买断整个应用包，最高 1,000,000 元。</Description>
              <FieldError />
            </TextField>
            <TextField
              isRequired
              isDisabled={disabled}
              value={value.rate}
              onChange={(rate) => onChange({ ...value, rate })}
            >
              <Label>发布者分成</Label>
              <InputGroup>
                <InputGroup.Input
                  inputMode="decimal"
                  placeholder="由运营审核确认"
                />
                <InputGroup.Suffix>%</InputGroup.Suffix>
              </InputGroup>
              <Description>0–100%，最多两位小数；其余为平台份额。</Description>
              <FieldError />
            </TextField>
          </>
        ) : null}
        <Description>
          整包授权，同商品后续兼容版本可升级。发布者分成审核通过后生效，免费领取不计收益。当前购买使用模拟支付，不发生实际扣款。
        </Description>
      </Fieldset.Group>
    </Fieldset>
  );
}
