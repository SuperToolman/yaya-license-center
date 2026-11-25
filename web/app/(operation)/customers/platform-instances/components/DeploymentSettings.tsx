"use client";
import { Button, Input } from "@heroui/react";
import { useState } from "react";
import { ModalShell } from "../../../../components/ui/ModalShell";
import { useOperationData } from "../../../../hooks/useOperationData";
import { useOperationMutation } from "../../../../hooks/useOperationMutation";
import { operationCommand } from "../../../../lib/operation-api";

export type Registry = {
  registryId: string;
  alias: string;
  url: string;
  username: string;
  hasPassword: boolean;
  mirrors: string[];
  isDefault: boolean;
};
const empty: Registry = {
  registryId: "",
  alias: "",
  url: "",
  username: "",
  hasPassword: false,
  mirrors: [],
  isDefault: false,
};

export default function DeploymentSettings({
  onClose,
}: {
  onClose: () => void;
}) {
  const query = useOperationData<Registry[]>("/api/platform-registries");
  const mutation = useOperationMutation();
  const [tab, setTab] = useState<"registry" | "proxy">("registry");
  const [editor, setEditor] = useState<Registry | null>(null);
  const [password, setPassword] = useState("");
  const [accelerator, setAccelerator] = useState<Registry | null>(null);
  const [deleting, setDeleting] = useState<Registry | null>(null);
  const [notice, setNotice] = useState("");
  const busy = mutation.pending;
  function openEditor(item: Registry) {
    mutation.clearError();
    setNotice("");
    setPassword("");
    setEditor({ ...item });
  }
  function back() {
    if (!busy) {
      setEditor(null);
      setAccelerator(null);
      setDeleting(null);
      mutation.clearError();
    }
  }
  async function save(item: Registry, secret?: string) {
    await mutation.run(
      () =>
        operationCommand(
          `/api/platform-registries${item.registryId ? `/${item.registryId}` : ""}`,
          {
            alias: item.alias,
            url: item.url,
            username: item.username,
            password: secret || undefined,
            mirrors: item.mirrors,
            isDefault: item.isDefault,
          },
          item.registryId ? "PUT" : "POST",
        ),
      () => {
        setEditor(null);
        setAccelerator(null);
        setPassword("");
        setNotice("连接设置已保存。离线镜像部署无需仓库连接。");
        query.refresh();
      },
    );
  }
  const title = editor
    ? editor.registryId
      ? "编辑镜像仓库"
      : "新建镜像仓库"
    : accelerator
      ? "加速器配置"
      : deleting
        ? "删除镜像仓库"
        : "设置";
  return (
    <ModalShell
      title={title}
      width={640}
      onClose={() => {
        if (!busy) onClose();
      }}
    >
      {editor ? (
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            void save(editor, password);
          }}
        >
          <label className="grid gap-1 text-sm">
            别名
            <Input
              aria-label="仓库别名"
              required
              maxLength={80}
              value={editor.alias}
              onChange={(e) => setEditor({ ...editor, alias: e.target.value })}
            />
          </label>
          <label className="grid gap-1 text-sm">
            仓库地址
            <Input
              aria-label="仓库地址"
              required
              type="url"
              placeholder="https://registry.example.com"
              disabled={editor.registryId === "dockerhub"}
              value={editor.url}
              onChange={(e) => setEditor({ ...editor, url: e.target.value })}
            />
          </label>
          <p className="text-xs text-muted">
            填写仓库服务地址，包含协议、主机和可选端口。此连接为仓库管理预留，不参与离线镜像部署。
          </p>
          <label className="grid gap-1 text-sm">
            用户名
            <Input
              aria-label="仓库用户名"
              autoComplete="off"
              value={editor.username}
              onChange={(e) =>
                setEditor({ ...editor, username: e.target.value })
              }
              placeholder="留空表示匿名访问"
            />
          </label>
          <label className="grid gap-1 text-sm">
            密码 / 访问令牌
            <Input
              aria-label="仓库密码"
              type="password"
              autoComplete="new-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={
                editor.hasPassword
                  ? "已保存，留空保留原密码"
                  : "填写仓库密码或访问令牌"
              }
            />
          </label>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={editor.isDefault}
              disabled={editor.isDefault && !!editor.registryId}
              onChange={(e) =>
                setEditor({ ...editor, isDefault: e.target.checked })
              }
            />
            设为默认仓库
          </label>
          <p className="text-xs text-muted">
            密码加密保存，不回显。清空用户名可改为匿名访问。
          </p>
          <div className="flex justify-end gap-2 border-t border-default pt-4">
            <Button variant="secondary" isDisabled={busy} onPress={back}>
              取消
            </Button>
            <Button type="submit" isDisabled={busy}>
              {busy ? "保存中…" : "保存"}
            </Button>
          </div>
        </form>
      ) : accelerator ? (
        <div className="grid gap-4">
          <p className="text-sm text-muted">
            此处仅保存加速器地址；离线镜像部署不会使用这些地址，也不会更改
            Docker daemon。
          </p>
          <Button
            variant="secondary"
            className="w-full border border-dashed"
            isDisabled={busy || accelerator.mirrors.length >= 10}
            onPress={() =>
              setAccelerator({
                ...accelerator,
                mirrors: [...accelerator.mirrors, ""],
              })
            }
          >
            ＋ 添加
          </Button>
          <div className="grid max-h-72 gap-3 overflow-y-auto">
            {accelerator.mirrors.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted">
                暂无已保存的加速器。
              </p>
            ) : null}
            {accelerator.mirrors.map((url, index) => (
              <div className="flex min-w-0 items-center gap-2" key={index}>
                <Input
                  aria-label={`加速器地址 ${index + 1}`}
                  placeholder="https://mirror.example.com"
                  value={url}
                  onChange={(e) =>
                    setAccelerator({
                      ...accelerator,
                      mirrors: accelerator.mirrors.map((v, i) =>
                        i === index ? e.target.value : v,
                      ),
                    })
                  }
                />
                <Button
                  isIconOnly
                  size="sm"
                  variant="ghost"
                  aria-label={`移除加速器 ${index + 1}`}
                  isDisabled={busy}
                  onPress={() =>
                    setAccelerator({
                      ...accelerator,
                      mirrors: accelerator.mirrors.filter(
                        (_, i) => i !== index,
                      ),
                    })
                  }
                >
                  ⊖
                </Button>
              </div>
            ))}
          </div>
          <div className="flex justify-end gap-2 border-t border-default pt-4">
            <Button variant="secondary" isDisabled={busy} onPress={back}>
              取消
            </Button>
            <Button
              isDisabled={busy || accelerator.mirrors.some((v) => !v.trim())}
              onPress={() => void save(accelerator)}
            >
              确定
            </Button>
          </div>
        </div>
      ) : deleting ? (
        <div className="grid gap-4">
          <p>确定删除镜像仓库“{deleting.alias}”？</p>
          <div className="flex justify-end gap-2">
            <Button variant="secondary" isDisabled={busy} onPress={back}>
              取消
            </Button>
            <Button
              isDisabled={busy}
              onPress={() =>
                void mutation.run(
                  () =>
                    operationCommand(
                      `/api/platform-registries/${deleting.registryId}`,
                      undefined,
                      "DELETE",
                    ),
                  () => {
                    setDeleting(null);
                    query.refresh();
                    setNotice("镜像仓库已删除。");
                  },
                )
              }
            >
              确认删除
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div
            role="tablist"
            aria-label="部署设置"
            className="mb-5 flex gap-6 border-b border-default"
          >
            {(
              [
                ["registry", "镜像仓库"],
                ["proxy", "代理"],
              ] as const
            ).map(([key, label]) => (
              <button
                key={key}
                role="tab"
                id={`deployment-tab-${key}`}
                aria-controls={`deployment-panel-${key}`}
                aria-selected={tab === key}
                className={`pb-3 text-sm font-medium ${tab === key ? "border-b-2 border-accent text-accent" : "text-muted"}`}
                onClick={() => {
                  setTab(key);
                  mutation.clearError();
                }}
              >
                {label}
              </button>
            ))}
          </div>
          <div
            role="tabpanel"
            id={`deployment-panel-${tab}`}
            aria-labelledby={`deployment-tab-${tab}`}
            className="min-h-72"
          >
            {tab === "proxy" ? (
              <div className="grid min-h-64 place-content-center gap-2 text-center">
                <p className="font-medium">代理设置</p>
                <p className="text-sm text-muted">
                  功能预留，暂不支持在此配置代理。
                </p>
              </div>
            ) : (
              <>
                <Button
                  size="sm"
                  isDisabled={query.loading || !!query.error || busy}
                  onPress={() => openEditor(empty)}
                >
                  ＋ 新建
                </Button>
                <div className="mt-4 overflow-x-auto">
                  <table
                    aria-label="镜像仓库连接"
                    className="w-full text-left text-sm"
                  >
                    <thead className="bg-default">
                      <tr>
                        <th className="p-3">别名</th>
                        <th className="p-3">仓库链接</th>
                        <th className="p-3">操作</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(query.data ?? []).map((item) => (
                        <tr
                          key={item.registryId}
                          className="border-b border-default align-top"
                        >
                          <td className="p-3">
                            {item.alias}
                            {item.isDefault ? (
                              <span className="ml-2 rounded bg-accent-soft px-1 text-xs text-accent">
                                默认
                              </span>
                            ) : null}
                          </td>
                          <td className="max-w-52 break-all p-3 text-muted">
                            {item.url}
                            {item.username ? (
                              <p className="mt-1 text-xs">
                                {item.username} · 已配置凭据
                              </p>
                            ) : null}
                          </td>
                          <td className="p-2">
                            <div className="flex flex-wrap gap-1">
                              {item.registryId === "dockerhub" ? (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  onPress={() => {
                                    mutation.clearError();
                                    setAccelerator({
                                      ...item,
                                      mirrors: [...item.mirrors],
                                    });
                                  }}
                                >
                                  加速器配置
                                </Button>
                              ) : null}
                              <Button
                                size="sm"
                                variant="ghost"
                                isDisabled={busy}
                                onPress={() => openEditor(item)}
                              >
                                编辑
                              </Button>
                              {!item.isDefault ? (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  isDisabled={busy}
                                  onPress={() =>
                                    void save({ ...item, isDefault: true })
                                  }
                                >
                                  设为默认
                                </Button>
                              ) : null}
                              {item.registryId !== "dockerhub" &&
                              !item.isDefault ? (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  isDisabled={busy}
                                  onPress={() => {
                                    mutation.clearError();
                                    setDeleting(item);
                                  }}
                                >
                                  删除
                                </Button>
                              ) : null}
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {query.loading ? (
                  <p className="py-6 text-sm text-muted">正在加载镜像仓库…</p>
                ) : null}
                {query.error ? (
                  <div className="mt-3">
                    <p role="alert" className="text-sm text-danger">
                      {query.error}
                    </p>
                    <Button size="sm" variant="ghost" onPress={query.refresh}>
                      重试
                    </Button>
                  </div>
                ) : null}
                <p className="mt-4 text-xs text-muted">
                  平台实例使用上传的离线镜像包，不需要仓库账号或加速器。连接信息仅作预留。
                </p>
              </>
            )}
          </div>
          <div className="mt-5 flex justify-end border-t border-default pt-4">
            <Button onPress={onClose} isDisabled={busy}>
              确定
            </Button>
          </div>
        </>
      )}
      {notice && !editor && !accelerator ? (
        <p role="status" className="mt-3 text-sm text-success">
          {notice}
        </p>
      ) : null}
      {mutation.error ? (
        <p role="alert" className="mt-3 text-sm text-danger">
          {mutation.error}
        </p>
      ) : null}
    </ModalShell>
  );
}
