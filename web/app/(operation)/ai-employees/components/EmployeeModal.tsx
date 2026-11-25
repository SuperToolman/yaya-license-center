"use client";
import {
  Button,
  Avatar,
  Card,
  CheckboxGroup,
  Checkbox as HeroCheckbox,
  Description,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Switch,
  TextArea,
  Tooltip,
  Surface,
} from "@heroui/react";
import { useEffect, useState } from "react";
import { EmployeeKnowledge } from "./EmployeeKnowledge";
import { LOW_CODE_ENGINEER_PROMPT, lowCodeEngineerTools } from "../lib/low-code-engineer";
import AvatarSetting from "../../../components/AvatarSetting";
import { formatDate } from "../../../lib/format";
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
  skill: "Skill 执行",
};
export type EmployeeSubmission = Pick<
  AiEmployeeProduct,
  | "id"
  | "title"
  | "description"
  | "category"
  | "priceCents"
  | "billingCycle"
  | "skillIds"
  | "systemPrompt"
  | "allowNetwork"
  | "allowedTools"
  | "applicationIds"
>;
const riskLabels: Record<PlatformTool["riskLevel"], string> = {
  read: "查看",
  write: "操作",
  external: "外部",
  destructive: "删除",
  execution: "本地执行",
};
function capabilityGroupLabel(
  category: string,
  group: string,
  riskLevel: PlatformTool["riskLevel"],
) {
  if (riskLevel === "destructive") return "高风险操作";
  if (category === "app") return "应用管理";
  if (category === "form")
    return group === "分析" ? "业务数据分析" : "表单搭建";
  if (category === "automation")
    return group === "分析" ? "自动化审查" : "自动化配置";
  if (category === "workflow") return riskLevel === "read" ? "工作流分析" : "审批流程配置";
  if (category === "plugin" || category === "script" || category === "skill")
    return "扩展能力";
  return group;
}
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
  onSubmit: (body: EmployeeSubmission, avatar?: File) => Promise<void>;
}) {
  const [avatar, setAvatar] = useState<File>();
  const [avatarPreview, setAvatarPreview] = useState<string>();
  useEffect(
    () => () => {
      if (avatarPreview) URL.revokeObjectURL(avatarPreview);
    },
    [avatarPreview],
  );
  const avatarSrc =
    avatarPreview ||
    (employee?.avatarUrl
      ? `/operation-api/api/ai-employees/${encodeURIComponent(employee.id)}/avatar?v=${encodeURIComponent(employee.avatarUrl)}`
      : undefined);
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
  const groupedTools = tools.reduce<
    Record<string, Record<string, PlatformTool[]>>
  >((categories, tool) => {
    const categoryGroups = (categories[tool.category] ??= {});
    const group = capabilityGroupLabel(
      tool.category,
      tool.group,
      tool.riskLevel,
    );
    (categoryGroups[group] ??= []).push(tool);
    return categories;
  }, {});
  const save = () =>
    onSubmit(
      {
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
        applicationIds: employee?.applicationIds ?? [],
      },
      avatar,
    );
  return (
    <Modal isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Backdrop isDismissable>
        <Modal.Container
          placement="center"
          scroll="inside"
          className="w-full sm:w-full"
        >
          <Modal.Dialog
            style={{ width: "min(1120px, 100%)", maxWidth: "none" }}
          >
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
                <div className="grid items-center gap-6 sm:grid-cols-[150px_minmax(0,1fr)]">
                  <section
                    className="flex justify-center"
                    aria-label="AI 员工头像设置"
                  >
                    <AvatarSetting
                      currentSrc={avatarSrc}
                      saveLabel="使用此头像"
                      isDisabled={submitting}
                      renderTrigger={(open) => (
                        <Tooltip>
                          <Tooltip.Trigger>
                            <Button
                              type="button"
                              variant="ghost"
                              isIconOnly
                              isDisabled={submitting}
                              aria-label="点击更换头像"
                              onPress={open}
                              className="size-[150px] shrink-0 rounded-full p-0"
                              style={{ width: 150, height: 150 }}
                            >
                              <Avatar
                                color="accent"
                                variant="soft"
                                className="size-[150px]"
                              >
                                {avatarSrc ? (
                                  <Avatar.Image
                                    src={avatarSrc}
                                    alt="AI 员工头像"
                                  />
                                ) : null}
                                <Avatar.Fallback>
                                  {form.title.slice(0, 1) || "AI"}
                                </Avatar.Fallback>
                              </Avatar>
                            </Button>
                          </Tooltip.Trigger>
                          <Tooltip.Content>点击更换头像</Tooltip.Content>
                        </Tooltip>
                      )}
                      onSave={async (file) => {
                        if (file.size > 5 * 1024 * 1024)
                          throw new Error("头像文件需小于 5 MB");
                        setAvatar(file);
                        setAvatarPreview(URL.createObjectURL(file));
                      }}
                    />
                  </section>
                  <div className="grid gap-4 sm:grid-cols-2">
                    <Field label="展示名称">
                      <Input
                        fullWidth
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
                        fullWidth
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
                        fullWidth
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
                            billingCycle: String(
                              key,
                            ) as typeof form.billingCycle,
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
                  </div>
                </div>
                <div className="grid gap-4">
                  <Field label="AI 员工描述" full>
                    <TextArea
                      fullWidth
                      rows={4}
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
                      fullWidth
                      rows={8}
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
                    className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3"
                    value={form.skillIds}
                    onChange={(skillIds) =>
                      setForm((current) => ({ ...current, skillIds }))
                    }
                    isDisabled={submitting}
                  >
                    {skills.map((skill) => (
                      <Card
                        key={skill.id}
                        variant="secondary"
                        className="min-w-0"
                      >
                        <Card.Content>
                          <HeroCheckbox
                            value={skill.id}
                            aria-label={`${skill.title}（${skill.id}）`}
                          >
                            <HeroCheckbox.Content>
                              <HeroCheckbox.Control>
                                <HeroCheckbox.Indicator />
                              </HeroCheckbox.Control>
                              <span>{skill.title}</span>
                            </HeroCheckbox.Content>
                            <Description className="space-y-1">
                              <span className="block break-all">
                                ID：{skill.id}
                              </span>
                              <span className="block">
                                版本：v{skill.version}
                              </span>
                              <span className="block">
                                更新时间：{formatDate(skill.updatedAt)}
                              </span>
                              {skill.description ? (
                                <span className="block">
                                  {skill.description}
                                </span>
                              ) : null}
                            </Description>
                          </HeroCheckbox>
                        </Card.Content>
                      </Card>
                    ))}
                  </CheckboxGroup>
                  {skills.length === 0 ? (
                    <Description>
                      暂无可封装的 Skill，请先在 Skills 管理中导入。
                    </Description>
                  ) : null}
                </section>
                <section>
                  <h3 className="text-sm font-semibold">员工能力</h3>
                  <Button type="button" variant="secondary" isDisabled={submitting}
                    onPress={() => setForm((current) => ({ ...current,
                      systemPrompt: LOW_CODE_ENGINEER_PROMPT,
                      allowedTools: [...lowCodeEngineerTools(tools.map((tool) => tool.id))],
                    }))}>
                    应用低代码开发工程师配置
                  </Button>
                  <Description>替换当前编辑中的提示词与能力选择；保存员工后发布版本，客户更新安装后生效。需要配套支持审批设计工具的平台版本。</Description>
                  <p className="mb-3 text-xs text-muted">
                    选择员工提供的能力。实际操作由使用者的 RBAC
                    权限决定，平台工具写入仍须用户确认。脚本与终端使用系统账户权限，
                    不受业务工具确认和 RBAC 的系统级约束；依赖安装不另行确认。
                  </p>
                  <div className="space-y-4">
                    {Object.entries(groupedTools).map(([category, groups]) => (
                      <div key={category}>
                        <h4 className="mb-2 text-xs font-semibold text-muted">
                          {categoryLabels[category] || category}
                        </h4>
                        <div className="space-y-2">
                          {Object.entries(groups).map(([group, groupTools]) => {
                            const groupIds = groupTools.map((tool) => tool.id);
                            const selectedCount = groupIds.filter((id) =>
                              form.allowedTools.includes(id),
                            ).length;
                            return (
                              <Surface
                                key={group}
                                className="shadow-lg rounded-xl p-2"
                              >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                  <Checkbox
                                    isSelected={
                                      selectedCount === groupIds.length
                                    }
                                    onChange={() => toggleMany(groupIds)}
                                    className="font-medium"
                                  >
                                    {group}
                                  </Checkbox>
                                  <span className="text-xs text-muted">
                                    {selectedCount}/{groupIds.length} 项已选择
                                  </span>
                                </div>
                                <details className="mt-2 border-t border-separator pt-2">
                                  <summary className="cursor-pointer text-xs text-muted">
                                    查看单项能力
                                  </summary>
                                  <div className="mt-2 grid gap-2 sm:grid-cols-3">
                                    {groupTools.map((tool) => (
                                      <Surface
                                        key={tool.id}
                                        variant="tertiary"
                                        className="rounded-xl border-l-4 border-accent bg-background p-2 shadow-lg"
                                      >
                                        <Checkbox
                                          isSelected={form.allowedTools.includes(
                                            tool.id,
                                          )}
                                          onChange={() =>
                                            toggle("allowedTools", tool.id)
                                          }
                                        >
                                          <strong className="block text-sm">
                                            {tool.title}
                                          </strong>
                                          <code className="block text-xs text-muted">
                                            {tool.id}
                                          </code>
                                          <small className="text-xs text-muted">
                                            {riskLabels[tool.riskLevel]} ·{" "}
                                            {tool.description}
                                          </small>
                                        </Checkbox>
                                      </Surface>
                                    ))}
                                  </div>
                                </details>
                              </Surface>
                            );
                          })}
                        </div>
                      </div>
                    ))}
                  </div>
                </section>
                <EmployeeKnowledge
                  employeeId={employee?.id}
                  disabled={submitting}
                />
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
