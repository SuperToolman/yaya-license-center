"use client";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import { Button, Card, Form } from "@heroui/react";
import type { ReactNode, FormEvent } from "react";
import ContainerStorageFields from "./ContainerStorageFields";
import ContainerPortFields from "./ContainerPortFields";
import ContainerEnvironmentFields from "./ContainerEnvironmentFields";
import type { ComponentProps } from "react";
export type ContainerConfiguration = {
  storage: ComponentProps<typeof ContainerStorageFields>;
  network: ComponentProps<typeof ContainerPortFields>;
  environment?: ComponentProps<typeof ContainerEnvironmentFields>;
};
/** Container configuration is shared; callers supply defaults and business constraints. */
export default function ContainerCreateForm({
  basic,
  configuration,
  networkExtra,
  environmentNotice,
  other,
  description,
  error,
  pending,
  disabled,
  onSubmit,
  onClose,
  submitLabel,
}: {
  basic: ReactNode;
  configuration: ContainerConfiguration;
  networkExtra?: ReactNode;
  environmentNotice?: ReactNode;
  other?: ReactNode;
  description: string;
  error: string | null;
  pending: boolean;
  disabled?: boolean;
  onSubmit: (event: FormEvent) => void;
  onClose: () => void;
  submitLabel: string;
}) {
  useErrorNotification(error);
  return (
    <Form onSubmit={onSubmit} className="flex flex-col gap-4">
      <p className="text-sm text-muted">{description}</p>
      {(
        [
          ["基础信息", basic],
          [
            "存储空间",
            <ContainerStorageFields key="storage" {...configuration.storage} />,
          ],
          [
            "网络配置",
            <div key="network" className="space-y-4">
              <ContainerPortFields {...configuration.network} />
              {networkExtra}
            </div>,
          ],
          [
            "其他",
            <div key="other" className="space-y-4">
              {environmentNotice}
              {configuration.environment ? (
                <ContainerEnvironmentFields {...configuration.environment} />
              ) : null}
              {other}
            </div>,
          ],
        ] as const
      ).map(([title, content]) => (
        <Card key={title} variant="secondary" className="min-w-0 p-4 sm:p-5">
          <h3 className="mb-4 text-sm font-semibold">{title}</h3>
          {content}
        </Card>
      ))}
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          variant="secondary"
          isDisabled={pending}
          onPress={onClose}
        >
          取消
        </Button>
        <Button type="submit" isDisabled={pending || disabled}>
          {pending ? "提交中…" : submitLabel}
        </Button>
      </div>
    </Form>
  );
}
