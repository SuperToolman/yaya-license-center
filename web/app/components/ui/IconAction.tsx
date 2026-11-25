"use client";
import { Button, Tooltip } from "@heroui/react";
import { type ReactNode } from "react";
export function IconAction({
  label,
  children,
  onPress,
  isDisabled,
  className,
}: {
  label: string;
  children: ReactNode;
  onPress: () => void;
  isDisabled?: boolean;
  className?: string;
}) {
  return (
    <Tooltip>
      <Tooltip.Trigger>
        <Button
          isDisabled={isDisabled}
          isIconOnly
          size="sm"
          variant="ghost"
          className={className}
          aria-label={label}
          onPress={onPress}
        >
          {children}
        </Button>
      </Tooltip.Trigger>
      <Tooltip.Content>{label}</Tooltip.Content>
    </Tooltip>
  );
}
