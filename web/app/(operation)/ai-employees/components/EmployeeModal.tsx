"use client";
import { useState } from "react";
import {
  Button,
  Checkbox as HeroCheckbox,
  CheckboxGroup,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Switch,
  TextArea,
} from "@heroui/react";
import AvatarSetting from "../../../components/AvatarSetting";
import { operationRequest } from "../../../lib/operation-api";
import type {
  AiEmployeeProduct,
  OperationSkill,
  PlatformTool,
} from "../../../lib/operation-types";

const categoryLabels: Record<string, string> = {
  app: "应用",
  form: "表单",
  automation: "集成自动化",
  workflow: "工作流",
  plugin: "插件",
  script: "脚本",
};
const riskLabels: Record<PlatformTool["riskLevel"], string> = {
  read: "查看",
  write: "操作",
  external: "外部",
  destructive: "删除",
};
function Checkbox(props: React.ComponentProps<typeof HeroCheckbox>) {
  const { children, ...rest } = props;
  return (
    <HeroCheckbox {...rest}>
      <HeroCheckbox.Content>
        <HeroCheckbox.Control>
          <HeroCheckbox.Indicator />
        </HeroCheckbox.Control>
        <span>{typeof children === "function" ? null : children}</span>
      </HeroCheckbox.Content>
    </HeroCheckbox>
  );
}

export default function EmployeeModal({
  employee,
  skills,
  tools,
  submitting,
  error,
  onClose,
  onSubmit,
}: {
  employee: AiEmployeeProduct | null;
  skills: OperationSkill[];
  tools: PlatformTool[];
  submitting: boolean;
  error: string;
  onClose: () => void;
  onSubmit: (body: unknown) => Promise<void>;
}) {
  const [form, setForm] = useState({
    id: employee?.id ?? "",
    title: employee?.title ?? "",
    description: employee?.description ?? "",
    category: employee?.category ?? "通用",
    price: employee ? String(employee.priceCents / 100) : "",
    billingCycle: employee?.billingCycle ?? "year",
    skillIds: employee?.skillIds ?? [],
    systemPrompt: employee?.systemPrompt ?? "",
    allowNetwork: employee?.allowNetwork ?? false,
    allowedTools: employee?.allowedTools ?? [],
  });
  const toggle = (key: "skillIds" | "allowedTools", id: string) =>
    setForm((current) => ({
      ...current,
      [key]: current[key].includes(id)
        ? current[key].filter((item) => item !== id)
        : [...current[key], id],
    }));
  const toggleMany = (ids: string[]) =>
    setForm((current) => ({
      ...current,
      allowedTools: ids.every((id) => current.allowedTools.includes(id))
        ? current.allowedTools.filter((id) => !ids.includes(id))
        : Array.from(new Set([...current.allowedTools, ...ids])),
    }));
  const save = () =>
    onSubmit({
      id: form.id.trim(),
      title: form.title.trim(),
      description: form.description,
      category: form.category,
      priceCents: Math.round(Number(form.price) * 100),
      billingCycle: form.billingCycle,
      skillIds: form.skillIds,
      systemPrompt: form.systemPrompt,
      allowNetwork: form.allowNetwork,
      allowedTools: form.allowedTools,
      applicationIds: [],
    });
  return (
    <Modal isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Backdrop isDismissable>
        <Modal.Container placement="center" scroll="inside" size="cover">
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>
                {employee ? "编辑 AI 员工" : "新增 AI 员工"}
              </Modal.Heading>
              <Modal.CloseTrigger aria-label="关闭" />
            </Modal.Header>
            <Modal.Body className="p-5">
              <form
                className="space-y-5"
                onSubmit={(event) => {
                  event.preventDefault();
                  void save();
                }}
              >
                <div className="grid gap-4 sm:grid-cols-2">
                  <Field label="展示名称">
                    <Input
                      aria-label="展示名称"
                      value={form.title}
                      onChange={(event) =>
                        setForm({ ...form, title: event.currentTarget.value })
                      }
                      required
                    />
                  </Field>
                  <Field label="分类">
                    <Input
                      aria-label="分类"
                      value={form.category}
                      onChange={(event) =>
                        setForm({
                          ...form,
                          category: event.currentTarget.value,
                        })
                      }
                      required
                    />
                  </Field>
                  <Field label="销售价格（元）">
                    <Input
                      aria-label="销售价格"
                      type="number"
                      value={form.price}
                      onChange={(event) =>
                        setForm({ ...form, price: event.currentTarget.value })
                      }
                      required
                    />
                  </Field>
                  <Field label="计费周期">
                    <Select
                      aria-label="计费周期"
                      selectedKey={form.billingCycle}
                      onSelectionChange={(key) =>
                        setForm({
                          ...form,
                          billingCycle: String(key) as typeof form.billingCycle,
                        })
                      }
                    >
                      <Select.Trigger>
                        <Select.Value />
                      </Select.Trigger>
                      <Select.Popover>
                        <ListBox>
                          <ListBox.Item id="month">按月</ListBox.Item>
                          <ListBox.Item id="year">按年</ListBox.Item>
                          <ListBox.Item id="one_time">一次性</ListBox.Item>
                        </ListBox>
                      </Select.Popover>
                    </Select>
                  </Field>
                  <Field label="AI 员工描述" full>
                    <TextArea
                      aria-label="AI 员工描述"
                      value={form.description}
                      onChange={(event) =>
                        setForm({
                          ...form,
                          description: event.currentTarget.value,
                        })
                      }
                    />
                  </Field>
                  <Field label="人格提示词" full>
                    <TextArea
                      aria-label="人格提示词"
                      value={form.systemPrompt}
                      onChange={(event) =>
                        setForm({
                          ...form,
                          systemPrompt: event.currentTarget.value,
                        })
                      }
                    />
                  </Field>
                </div>
                {employee ? (
                  <AvatarSetting
                    label="更新头像"
                    isDisabled={submitting}
                    onSave={async (file) => {
                      const result = await operationRequest<AiEmployeeProduct>(
                        `/api/ai-employees/${encodeURIComponent(employee.id)}/avatar`,
                        {
                          method: "PUT",
                          headers: { "content-type": file.type },
                          body: file,
                        },
                      );
                      if (!result.response.ok)
                        throw new Error(
                          result.payload.message || "头像保存失败",
                        );
                    }}
                  />
                ) : null}
                <Switch
                  isSelected={form.allowNetwork}
                  onChange={(allowNetwork) =>
                    setForm({ ...form, allowNetwork })
                  }
                >
                  允许访问网络
                </Switch>
                <section>
                  <h3 className="mb-2 text-sm font-semibold">封装 Skills</h3>
                  <CheckboxGroup
                    aria-label="封装 Skills"
                    className="grid gap-2 sm:grid-cols-2"
                  >
                    {skills.map((skill) => (
                      <Checkbox
                        key={skill.id}
                        isSelected={form.skillIds.includes(skill.id)}
                        onChange={() => toggle("skillIds", skill.id)}
                        className="border border-border p-3"
                      >
                        {skill.title}
                      </Checkbox>
                    ))}
                  </CheckboxGroup>
                </section>
                <section>
                  <h3 className="text-base font-semibold">AI 员工平台能力</h3>
                  <p className="mb-3 text-xs text-muted">
                    按能力类别、分组和查看、操作、删除权限授权。
                  </p>
                  {Array.from(new Set(tools.map((tool) => tool.category))).map(
                    (category) => {
                      const categoryTools = tools.filter(
                        (tool) => tool.category === category,
                      );
                      const categoryIds = categoryTools.map((tool) => tool.id);
                      return (
                        <div
                          key={category}
                          className="mb-3 border border-separator bg-surface-secondary p-3"
                        >
                          <Checkbox
                            isSelected={categoryIds.every((id) =>
                              form.allowedTools.includes(id),
                            )}
                            onChange={() => toggleMany(categoryIds)}
                            className="text-base font-semibold"
                          >
                            {categoryLabels[category] || category}
                          </Checkbox>
                          {Array.from(
                            new Set(categoryTools.map((tool) => tool.group)),
                          ).map((group) => {
                            const groupTools = categoryTools.filter(
                              (tool) => tool.group === group,
                            );
                            const groupIds = groupTools.map((tool) => tool.id);
                            return (
                              <div
                                key={group}
                                className="mt-3 border-l-3 border-accent/40 pl-3"
                              >
                                <Checkbox
                                  isSelected={groupIds.every((id) =>
                                    form.allowedTools.includes(id),
                                  )}
                                  onChange={() => toggleMany(groupIds)}
                                  className="font-medium"
                                >
                                  {group}
                                </Checkbox>
                                <div className="mt-2 grid gap-2 sm:grid-cols-2">
                                  {groupTools.map((tool) => (
                                    <Checkbox
                                      key={tool.id}
                                      isSelected={form.allowedTools.includes(
                                        tool.id,
                                      )}
                                      onChange={() =>
                                        toggle("allowedTools", tool.id)
                                      }
                                      className="border-l-4 border-accent bg-background p-2"
                                    >
                                      <strong className="block text-sm">
                                        {tool.title}
                                      </strong>
                                      <small className="text-xs text-muted">
                                        {riskLabels[tool.riskLevel]} ·{" "}
                                        {tool.description}
                                      </small>
                                    </Checkbox>
                                  ))}
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      );
                    },
                  )}
                </section>
                {error ? <p className="text-danger">{error}</p> : null}
                <div className="flex justify-end gap-2">
                  <Button type="button" variant="ghost" onPress={onClose}>
                    取消
                  </Button>
                  <Button type="submit" isPending={submitting}>
                    保存 AI 员工
                  </Button>
                </div>
              </form>
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
function Field({
  label,
  full,
  children,
}: {
  label: string;
  full?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className={full ? "sm:col-span-2" : undefined}>
      <Label className="mb-1.5 block">{label}</Label>
      {children}
    </div>
  );
}
