"use client";
import { Modal } from "@heroui/react";
import { type ReactNode } from "react";
export function ModalShell({
  title,
  children,
  onClose,
  width = 512,
  height,
  bodyClassName = "p-5",
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  width?: number;
  height?: number;
  bodyClassName?: string;
}) {
  return (
    <Modal isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Backdrop isDismissable>
        <Modal.Container
          placement="center"
          scroll="inside"
          className="w-full sm:w-full"
        >
          <Modal.Dialog style={{ width: "100%", maxWidth: width, height }}>
            <Modal.Header>
              <Modal.Heading>{title}</Modal.Heading>
              <Modal.CloseTrigger aria-label="关闭" />
            </Modal.Header>
            <Modal.Body className={bodyClassName}>{children}</Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
