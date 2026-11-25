"use client";
import { Description, FieldError, Label, ListBox, Select } from "@heroui/react";
export function ApplicationSelect({
  label,
  value,
  onChange,
  options,
  disabled,
  required,
  placeholder = "请选择",
  description,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { id: string; label: string }[];
  disabled?: boolean;
  required?: boolean;
  placeholder?: string;
  description?: string;
}) {
  return (
    <Select
      fullWidth
      isRequired={required}
      isDisabled={disabled}
      selectedKey={value || null}
      onSelectionChange={(key) => onChange(String(key ?? ""))}
      placeholder={placeholder}
    >
      <Label>{label}</Label>
      <Select.Trigger>
        <Select.Value />
        <Select.Indicator />
      </Select.Trigger>
      <Select.Popover>
        <ListBox>
          {options.map((option) => (
            <ListBox.Item
              key={option.id}
              id={option.id}
              textValue={option.label}
            >
              {option.label}
              <ListBox.ItemIndicator />
            </ListBox.Item>
          ))}
        </ListBox>
      </Select.Popover>
      {description ? <Description>{description}</Description> : null}
      <FieldError />
    </Select>
  );
}
