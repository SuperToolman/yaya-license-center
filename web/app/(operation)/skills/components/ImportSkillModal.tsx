"use client";

import { CloudArrowUpIn } from "@gravity-ui/icons";
import {
  Alert,
  Button,
  Checkbox,
  Description,
  Label,
  Modal,
  Spinner,
} from "@heroui/react";
import { useRef, useState } from "react";
import {
  packageSkillDirectory,
  readDroppedSkill,
  type SkillFile,
} from "../lib/import-skill";

export default function ImportSkillModal({
  error,
  onClose,
  onImport,
  title = "导入技能",
  description,
}: {
  error: string;
  onClose: () => void;
  onImport: (file: File) => Promise<void>;
  title?: string;
  description?: string;
}) {
  const archiveInput = useRef<HTMLInputElement>(null);
  const directoryInput = useRef<HTMLInputElement>(null);
  const busy = useRef(false);
  const [pending, setPending] = useState(false);
  const [localError, setLocalError] = useState("");
  const [dragging, setDragging] = useState(false);

  async function submit(prepare: () => Promise<File>) {
    if (busy.current) return;
    busy.current = true;
    setPending(true);
    setLocalError("");
    try {
      const file = await prepare();
      if (!/\.zip$/i.test(file.name))
        throw new Error("压缩包导入仅支持 .zip 文件");
      if (!file.size || file.size > 10 * 1024 * 1024)
        throw new Error("Skill 压缩包不能为空且不得超过 10 MB");
      await onImport(file);
    } catch (cause) {
      setLocalError(
        cause instanceof Error ? cause.message : "无法读取导入文件，请重试",
      );
    } finally {
      busy.current = false;
      setPending(false);
    }
  }

  return (
    <Modal isOpen onOpenChange={(open) => !open && !busy.current && onClose()}>
      <Modal.Backdrop
        isDismissable={!pending}
        isKeyboardDismissDisabled={pending}
      >
        <Modal.Container placement="center" size="sm" scroll="inside">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>{title}</Modal.Heading>
              <Modal.CloseTrigger aria-label="关闭" isDisabled={pending} />
            </Modal.Header>
            <Modal.Body className="space-y-4">
              {description ? <Description>{description}</Description> : null}
              <input
                ref={archiveInput}
                hidden
                type="file"
                accept=".zip,application/zip"
                aria-label="ZIP 文件"
                disabled={pending}
                onChange={(event) => {
                  const file = event.currentTarget.files?.[0];
                  event.currentTarget.value = "";
                  if (file) void submit(async () => file);
                }}
              />
              <input
                ref={directoryInput}
                hidden
                type="file"
                {...{ webkitdirectory: "" }}
                multiple
                aria-label="Skill 文件夹"
                disabled={pending}
                onChange={(event) => {
                  const files: SkillFile[] = Array.from(
                    event.currentTarget.files ?? [],
                    (file) => ({ file, path: file.webkitRelativePath }),
                  );
                  event.currentTarget.value = "";
                  if (files.length)
                    void submit(() => packageSkillDirectory(files));
                }}
              />
              {/* HeroUI has no drop zone component; native drag events provide directory access. */}
              <div
                onDragOver={(event) => {
                  event.preventDefault();
                  if (!busy.current) setDragging(true);
                }}
                onDragLeave={(event) => {
                  if (
                    !event.currentTarget.contains(
                      event.relatedTarget as Node | null,
                    )
                  )
                    setDragging(false);
                }}
                onDrop={(event) => {
                  event.preventDefault();
                  setDragging(false);
                  if (busy.current) return;
                  const files = Array.from(event.dataTransfer.files);
                  const items = Array.from(event.dataTransfer.items);
                  // Start reading entries before the drop event loses access to the data store.
                  if (files.length === 1 && /\.zip$/i.test(files[0].name))
                    void submit(async () => files[0]);
                  else
                    void submit(async () =>
                      packageSkillDirectory(await readDroppedSkill(items)),
                    );
                }}
              >
                <Button
                  variant="tertiary"
                  fullWidth
                  isDisabled={pending}
                  className={`min-h-36 flex-col gap-3 border border-dashed ${dragging ? "border-accent" : "border-border"}`}
                  onPress={() => archiveInput.current?.click()}
                >
                  {pending ? (
                    <Spinner size="sm" />
                  ) : (
                    <CloudArrowUpIn className="size-8" />
                  )}
                  {pending
                    ? "正在处理并导入…"
                    : "拖拽 ZIP / 文件夹到此处，或点击上传 ZIP"}
                </Button>
              </div>
              <Checkbox isDisabled>
                <Checkbox.Content>
                  <Checkbox.Control>
                    <Checkbox.Indicator />
                  </Checkbox.Control>
                  <Label>非高风险自动安装</Label>
                </Checkbox.Content>
                <Description>暂未接入风险评估，当前仅导入技能。</Description>
              </Checkbox>
              <div className="space-y-2">
                <Label>文件要求</Label>
                <Description>
                  <ul className="list-disc space-y-2 pl-5">
                    <li>
                      文件夹或 .zip 包需要包含 SKILL.md 文件；文件夹的 SKILL.md
                      请放在根目录。
                    </li>
                    <li>SKILL.md 应包含 YAML 格式的技能名称和描述。</li>
                    <li>
                      ZIP 不超过 10 MB，文件夹不超过 256 个文件、20
                      MB（打包后不超过 10 MB）。
                    </li>
                  </ul>
                </Description>
              </div>
              {localError || error ? (
                <Alert status="danger">
                  <Alert.Content>
                    <Alert.Description>{localError || error}</Alert.Description>
                  </Alert.Content>
                </Alert>
              ) : null}
            </Modal.Body>
            <Modal.Footer className="justify-start gap-2">
              <Button
                variant="secondary"
                isDisabled={pending}
                onPress={() => archiveInput.current?.click()}
              >
                选择 ZIP 文件
              </Button>
              <Button
                variant="secondary"
                isDisabled={pending}
                onPress={() => directoryInput.current?.click()}
              >
                选择文件夹
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
