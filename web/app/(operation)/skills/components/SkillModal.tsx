"use client";
import { CloudArrowUpIn } from "@gravity-ui/icons";
import {
  Alert,
  Button,
  Form,
  Input,
  Label,
  Modal,
  TextArea,
  TextField,
} from "@heroui/react";
import { useId, useState } from "react";
import type { OperationSkill } from "../../../lib/operation-types";
import ImportSkillModal from "./ImportSkillModal";
export default function SkillModal({
  skill,
  submitting,
  error,
  onClose,
  onSubmit,
  onArchiveUpload,
}: {
  skill: OperationSkill;
  submitting: boolean;
  error: string;
  onClose: () => void;
  onSubmit: (body: unknown) => Promise<void>;
  onArchiveUpload: (file: File) => Promise<void>;
}) {
  const formId = useId();
  const [importOpen, setImportOpen] = useState(false);
  const environmentVariables = skill.environmentVariables ?? [];
  const [form, setForm] = useState({
    id: skill.id,
    title: skill.title,
    source: skill.source,
    description: skill.description,
    instructions: skill.instructions,
    environmentValues: Object.fromEntries(
      environmentVariables.map(({ name }) => [
        name,
        skill.environmentValues?.[name] ?? "",
      ]),
    ),
  });
  return (
    <>
      <Modal isOpen onOpenChange={(open) => !open && onClose()}>
        <Modal.Backdrop isDismissable>
          {/* The wide editor uses local sizing; HeroUI manages scrolling and appearance. */}
          <Modal.Container
            placement="center"
            scroll="inside"
            className="w-full sm:w-full"
          >
            <Modal.Dialog className="w-[960px] max-w-full">
              <Modal.Header>
                <Modal.Heading>编辑 Skill</Modal.Heading>
                <Modal.CloseTrigger aria-label="关闭" />
              </Modal.Header>
              <Modal.Body>
                <Form
                  id={formId}
                  className="grid gap-4 sm:grid-cols-2"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void onSubmit(form);
                  }}
                >
                  <TextField isRequired fullWidth>
                    <Label>Skill 名称</Label>
                    <Input
                      value={form.title}
                      onChange={(event) =>
                        setForm({ ...form, title: event.currentTarget.value })
                      }
                      autoFocus
                    />
                  </TextField>
                  <TextField isReadOnly fullWidth>
                    <Label>包名</Label>
                    <Input value={skill.packageName || skill.id} />
                  </TextField>
                  <TextField isDisabled fullWidth>
                    <Label>来源</Label>
                    <Input value={form.source} />
                  </TextField>
                  <TextField isReadOnly fullWidth>
                    <Label>版本</Label>
                    <Input value={skill.version} />
                  </TextField>
                  <TextField fullWidth className="sm:col-span-2">
                    <Label>能力说明</Label>
                    <TextArea
                      value={form.description}
                      onChange={(event) =>
                        setForm({
                          ...form,
                          description: event.currentTarget.value,
                        })
                      }
                    />
                  </TextField>
                  <TextField isRequired fullWidth className="sm:col-span-2">
                    <Label>执行说明</Label>
                    <TextArea
                      className="min-h-40 font-mono"
                      value={form.instructions}
                      onChange={(event) =>
                        setForm({
                          ...form,
                          instructions: event.currentTarget.value,
                        })
                      }
                    />
                  </TextField>
                  {environmentVariables.map((variable) => (
                    <TextField
                      key={variable.name}
                      fullWidth
                      className="sm:col-span-2"
                    >
                      <Label>{variable.name}</Label>
                      <Input
                        name={variable.name}
                        type={variable.secret ? "password" : "text"}
                        autoComplete="off"
                        value={form.environmentValues[variable.name] ?? ""}
                        onChange={(event) =>
                          setForm({
                            ...form,
                            environmentValues: {
                              ...form.environmentValues,
                              [variable.name]: event.currentTarget.value,
                            },
                          })
                        }
                      />
                    </TextField>
                  ))}
                  {error ? (
                    <Alert status="danger" className="sm:col-span-2">
                      <Alert.Content>
                        <Alert.Description>{error}</Alert.Description>
                      </Alert.Content>
                    </Alert>
                  ) : null}
                </Form>
              </Modal.Body>
              <Modal.Footer>
                <Button type="button" variant="ghost" onPress={onClose}>
                  取消
                </Button>
                <Button
                  type="button"
                  variant="secondary"
                  isDisabled={submitting}
                  onPress={() => setImportOpen(true)}
                >
                  <CloudArrowUpIn className="h-4 w-4" />
                  导入新版本
                </Button>
                <Button type="submit" form={formId} isPending={submitting}>
                  保存 Skill
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
      {importOpen ? (
        <ImportSkillModal
          title="导入新版本"
          description={`为 ${skill.title}（${skill.id}）导入新版本，成功后替换当前技能文件。`}
          error={error}
          onClose={() => setImportOpen(false)}
          onImport={onArchiveUpload}
        />
      ) : null}
    </>
  );
}
