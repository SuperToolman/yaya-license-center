"use client";
import {
  Breadcrumbs,
  Button,
  Dropdown,
  Input,
  Label,
  Table,
  toast,
} from "@heroui/react";
import {
  Folder,
  FileText,
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  House,
  ArrowDownToLine,
  Ellipsis,
} from "@gravity-ui/icons";
import { useRef, useState } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import OperationPageHeader from "../OperationPageHeader";
import { useOperation } from "../OperationProvider";
import { DataPanel } from "../ui/DataPanel";
import { DataTable } from "../ui/DataTable";
import { ModalShell } from "../ui/ModalShell";
import { IconAction } from "../ui/IconAction";
import { useOperationData } from "../../hooks/useOperationData";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import {
  operationCommand,
  operationData,
  operationError,
  operationUrl,
} from "../../lib/operation-api";

type Entry = {
  name: string;
  path: string;
  kind: string;
  size: number | null;
  modifiedAt: number | null;
  mode: string;
  owner: string;
  revision: string;
  canDelete?: boolean;
};
type Properties = Entry & { absolutePath: string; createdAt: number | null };
type Listing = {
  root: string;
  path: string;
  items: Entry[];
  total: number;
  page: number;
  pageSize: number;
};
const base = "/api/ops/local/files";
const sizeText = (n: number | null) =>
  n === null
    ? "—"
    : n < 1024
      ? `${n} B`
      : n < 1024 ** 2
        ? `${(n / 1024).toFixed(1)} KiB`
        : n < 1024 ** 3
          ? `${(n / 1024 ** 2).toFixed(1)} MiB`
          : `${(n / 1024 ** 3).toFixed(1)} GiB`;
export default function FilesPage() {
  const { currentUser } = useOperation();
  useErrorNotification(
    currentUser && currentUser.role !== "platform_admin"
      ? "当前账号没有文件管理权限。"
      : "",
  );
  if (currentUser?.role !== "platform_admin")
    return <p className="text-sm text-muted">文件管理仅向平台管理员开放。</p>;
  return <FileBrowser />;
}
function FileBrowser() {
  const pathname = usePathname(),
    params = useSearchParams();
  const path = params.get("path") || "/",
    search = params.get("search") || "",
    page = Math.max(0, Number(params.get("page")) || 0);
  const [address, setAddress] = useState<string | null>(null);
  const [jumpOpen, setJumpOpen] = useState(false);
  const [forward, setForward] = useState<{
    from: string;
    paths: string[];
  } | null>(null);
  const segments = path.split("/").filter(Boolean);
  const crumbs = [
    { name: "/", path: "/" },
    ...segments.map((name, i) => ({
      name,
      path: `/${segments.slice(0, i + 1).join("/")}`,
    })),
  ];
  const forwardPaths = forward?.from === path ? forward.paths : [];
  const [filter, setFilter] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [create, setCreate] = useState<"mkdir" | "create" | null>(null),
    [name, setName] = useState("");
  const [deleting, setDeleting] = useState<Entry | null>(null);
  const [renaming, setRenaming] = useState<Entry | null>(null);
  const [properties, setProperties] = useState<Properties | null>(null);
  const [sizes, setSizes] = useState<Record<string, number>>({});
  const [calculating, setCalculating] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ name: string; text: string } | null>(
    null,
  );
  const [progress, setProgress] = useState<number | null>(null);
  const uploadInput = useRef<HTMLInputElement>(null);
  const listing = useOperationData<Listing>(
    `${base}?${new URLSearchParams({ path, search, page: String(page) })}`,
  );
  useErrorNotification(listing.error, listing.loading);
  const navigate = (
    target: string,
    query = search,
    nextPage = 0,
    remainingForward?: string[],
  ) => {
    if (busy) return;
    target = `/${target.split("/").filter(Boolean).join("/")}`;
    if (target !== path) {
      const returningUp = target === "/" || path.startsWith(`${target}/`);
      setForward({
        from: target,
        paths: remainingForward ?? (returningUp ? [path, ...forwardPaths] : []),
      });
    }
    setAddress(null);
    setFilter(null);
    // Next's native history integration updates search params without an RSC navigation.
    window.history.pushState(
      null,
      "",
      `${pathname}?${new URLSearchParams({ path: target, search: query, page: String(nextPage) })}`,
    );
  };
  const run = async (action: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    try {
      await action();
    } catch (e) {
      toast.danger(operationError(e), { timeout: 8000 });
    } finally {
      setBusy(false);
    }
  };
  const upload = async (file: File) => {
    if (file.size > 500 * 1024 ** 2) {
      toast.danger("单个文件不得超过 500 MiB", { timeout: 8000 });
      return;
    }
    await run(async () => {
      setProgress(0);
      try {
        await new Promise<void>((resolve, reject) => {
          const req = new XMLHttpRequest();
          req.open(
            "POST",
            operationUrl(`${base}/upload?${new URLSearchParams({ path })}`),
          );
          req.withCredentials = true;
          req.timeout = 15 * 60 * 1000;
          req.upload.onprogress = (e) => {
            if (e.lengthComputable)
              setProgress(Math.round((e.loaded / e.total) * 100));
          };
          req.onerror = () => reject(Error("上传连接失败"));
          req.ontimeout = () => reject(Error("上传超时，请刷新核对文件状态"));
          req.onload = () => {
            if (req.status >= 200 && req.status < 300) resolve();
            else {
              try {
                reject(
                  Error(JSON.parse(req.responseText).message || "上传失败"),
                );
              } catch {
                reject(Error(`上传失败（HTTP ${req.status}）`));
              }
            }
          };
          const data = new FormData();
          data.append("file", file);
          req.send(data);
        });
        toast.success("文件上传完成");
        setSizes({});
        listing.refresh();
      } finally {
        setProgress(null);
      }
    });
  };
  const open = (item: Entry) => {
    if (item.kind === "directory") {
      navigate(item.path, "");
      return;
    }
    void run(async () => {
      const data = await operationData<{ text: string }>(
        `${base}/preview?${new URLSearchParams({ path: item.path })}`,
      );
      setPreview({ name: item.name, text: data.text });
    });
  };
  const calculateSize = (item: Entry) =>
    void run(async () => {
      setCalculating(item.path);
      try {
        const data = await operationData<{ bytes: number }>(
          `${base}/size?${new URLSearchParams({ path: item.path })}`,
        );
        setSizes((previous) => ({
          ...previous,
          [`${item.path}:${item.revision}`]: data.bytes,
        }));
      } finally {
        setCalculating(null);
      }
    });
  const download = (item: Entry) =>
    void run(async () => {
      await operationData(
        `${base}/download?${new URLSearchParams({ path: item.path, check: "true" })}`,
      );
      const link = document.createElement("a");
      link.href = operationUrl(
        `${base}/download?${new URLSearchParams({ path: item.path })}`,
      );
      link.download = item.name;
      document.body.appendChild(link);
      link.click();
      link.remove();
    });
  const copyPath = (item: Entry) =>
    void run(async () => {
      const data = await operationData<Properties>(
        `${base}/properties?${new URLSearchParams({ path: item.path })}`,
      );
      let copied = false;
      if (navigator.clipboard) {
        try {
          await navigator.clipboard.writeText(data.absolutePath);
          copied = true;
        } catch {
          /* HTTP deployments use the selection fallback. */
        }
      }
      if (!copied) {
        const input = document.createElement("textarea");
        input.value = data.absolutePath;
        input.style.position = "fixed";
        input.style.opacity = "0";
        document.body.appendChild(input);
        try {
          input.select();
          if (!document.execCommand("copy"))
            throw Error("无法复制路径，请在属性中手动复制");
        } finally {
          input.remove();
        }
      }
      toast.success("路径已复制");
    });
  return (
    <>
      <OperationPageHeader
        title="文件"
        eyebrow="FILES"
        description="管理当前运行环境中的文件和目录。"
        loading={listing.loading || busy}
        onRefresh={() => {
          setSizes({});
          listing.refresh();
        }}
      />
      <DataPanel title="文件管理">
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <IconAction
            label="上级目录"
            isDisabled={busy || path === "/"}
            onPress={() =>
              navigate(
                path.split("/").filter(Boolean).slice(0, -1).join("/") || "/",
                "",
              )
            }
          >
            <ArrowLeft />
          </IconAction>
          {forwardPaths.length > 0 ? (
            <IconAction
              label="下级目录"
              isDisabled={busy}
              onPress={() =>
                navigate(forwardPaths[0], "", 0, forwardPaths.slice(1))
              }
            >
              <ArrowRight />
            </IconAction>
          ) : null}
          <IconAction
            label="根目录"
            isDisabled={busy}
            onPress={() => navigate("/", "")}
          >
            <House />
          </IconAction>
          <Breadcrumbs
            aria-label="当前路径"
            className="min-w-0 flex-1 overflow-x-auto"
            onAction={(key) => navigate(String(key), "")}
          >
            {crumbs.map((crumb) => (
              <Breadcrumbs.Item
                key={crumb.path}
                id={crumb.path}
                isDisabled={busy}
              >
                {crumb.name}
              </Breadcrumbs.Item>
            ))}
          </Breadcrumbs>
          <IconAction
            label="跳转路径"
            isDisabled={busy}
            onPress={() => {
              setAddress(path);
              setJumpOpen(true);
            }}
          >
            <ArrowUpRight />
          </IconAction>
        </div>
        <div className="mb-4 flex flex-wrap items-center gap-2">
          <Button
            isDisabled={busy || !!listing.error}
            onPress={() => {
              setName("");
              setCreate("mkdir");
            }}
          >
            新建目录
          </Button>
          <Button
            variant="secondary"
            isDisabled={busy || !!listing.error}
            onPress={() => {
              setName("");
              setCreate("create");
            }}
          >
            新建文件
          </Button>
          <Button
            variant="secondary"
            isDisabled={busy || !!listing.error}
            onPress={() => uploadInput.current?.click()}
          >
            上传文件
          </Button>
          <input
            ref={uploadInput}
            className="hidden"
            type="file"
            aria-label="选择上传文件"
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (file) void upload(file);
            }}
          />
          <form
            className="ml-auto flex gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              navigate(path, filter ?? search);
            }}
          >
            <Input
              aria-label="搜索当前目录"
              placeholder="搜索当前目录"
              value={filter ?? search}
              onChange={(e) => setFilter(e.target.value)}
            />
            <Button type="submit" variant="secondary" isDisabled={busy}>
              搜索
            </Button>
          </form>
        </div>
        {progress !== null ? (
          <p role="status" className="mb-3 text-accent">
            上传进度 {progress}%{progress === 100 ? "，正在保存…" : ""}
          </p>
        ) : null}
        <DataTable
          label="当前目录文件"
          columns={[
            "名称",
            "权限",
            "用户 / 用户组 ID",
            "大小",
            "修改时间",
            "操作",
          ]}
          empty={
            listing.loading
              ? "正在读取目录…"
              : listing.error
                ? "目录读取失败，可切换目录或重试"
                : "当前目录没有匹配文件"
          }
        >
          {(listing.error ? [] : (listing.data?.items ?? [])).map((item) => (
            <Table.Row key={item.path}>
              <Table.Cell>
                <Button
                  variant="ghost"
                  isDisabled={
                    busy || !["file", "directory"].includes(item.kind)
                  }
                  onPress={() => open(item)}
                >
                  {item.kind === "directory" ? (
                    <Folder className="text-warning" />
                  ) : (
                    <FileText />
                  )}
                  {item.name}
                </Button>
              </Table.Cell>
              <Table.Cell>{item.mode}</Table.Cell>
              <Table.Cell>{item.owner}</Table.Cell>
              <Table.Cell>
                {item.kind === "directory" ? (
                  <div className="flex items-center gap-1">
                    {sizes[`${item.path}:${item.revision}`] !== undefined ? (
                      <span>
                        {sizeText(sizes[`${item.path}:${item.revision}`])}
                      </span>
                    ) : null}
                    <Button
                      size="sm"
                      variant="ghost"
                      isDisabled={busy}
                      isPending={calculating === item.path}
                      aria-label={`计算目录大小 ${item.name}`}
                      onPress={() => calculateSize(item)}
                    >
                      {sizes[`${item.path}:${item.revision}`] !== undefined
                        ? "重新计算"
                        : "计算"}
                    </Button>
                  </div>
                ) : (
                  sizeText(item.size)
                )}
              </Table.Cell>
              <Table.Cell>
                {item.modifiedAt
                  ? new Date(item.modifiedAt).toLocaleString("zh-CN")
                  : "—"}
              </Table.Cell>
              <Table.Cell>
                <div className="flex gap-1">
                  <Button
                    isIconOnly
                    size="sm"
                    variant="ghost"
                    aria-label={`打开 ${item.name}`}
                    isDisabled={
                      busy || !["file", "directory"].includes(item.kind)
                    }
                    onPress={() => open(item)}
                  >
                    <ArrowUpRight />
                  </Button>
                  {item.kind === "file" ? (
                    <a
                      className="button button--icon-only button--sm button--ghost"
                      aria-label={`下载 ${item.name}`}
                      title="下载"
                      aria-disabled={busy}
                      onClick={(event) => {
                        event.preventDefault();
                        if (!busy) download(item);
                      }}
                      href={operationUrl(
                        `${base}/download?${new URLSearchParams({ path: item.path })}`,
                      )}
                    >
                      <ArrowDownToLine />
                    </a>
                  ) : (
                    <Button
                      isIconOnly
                      size="sm"
                      variant="ghost"
                      aria-label={`下载 ${item.name}`}
                      isDisabled
                    >
                      <ArrowDownToLine />
                    </Button>
                  )}
                  <Dropdown>
                    <Button
                      isIconOnly
                      size="sm"
                      variant="ghost"
                      aria-label={`更多 ${item.name}`}
                      isDisabled={busy}
                    >
                      <Ellipsis />
                    </Button>
                    <Dropdown.Popover placement="bottom end">
                      <Dropdown.Menu aria-label={`${item.name}的更多操作`}>
                        <Dropdown.Item
                          id="delete"
                          textValue="删除"
                          className="text-danger"
                          isDisabled={
                            !["file", "directory"].includes(item.kind) ||
                            item.canDelete === false
                          }
                          onAction={() => setDeleting(item)}
                        >
                          <Label>删除</Label>
                        </Dropdown.Item>
                        <Dropdown.Item
                          id="rename"
                          textValue="重命名"
                          isDisabled={
                            !["file", "directory"].includes(item.kind) ||
                            item.canDelete === false
                          }
                          onAction={() => {
                            setName(item.name);
                            setRenaming(item);
                          }}
                        >
                          <Label>重命名</Label>
                        </Dropdown.Item>
                        <Dropdown.Item
                          id="copy"
                          textValue="复制路径"
                          isDisabled={
                            !["file", "directory"].includes(item.kind)
                          }
                          onAction={() => copyPath(item)}
                        >
                          <Label>复制路径</Label>
                        </Dropdown.Item>
                        <Dropdown.Item
                          id="properties"
                          textValue="属性"
                          isDisabled={
                            !["file", "directory"].includes(item.kind)
                          }
                          onAction={() =>
                            void run(async () =>
                              setProperties(
                                await operationData<Properties>(
                                  `${base}/properties?${new URLSearchParams({ path: item.path })}`,
                                ),
                              ),
                            )
                          }
                        >
                          <Label>属性</Label>
                        </Dropdown.Item>
                      </Dropdown.Menu>
                    </Dropdown.Popover>
                  </Dropdown>
                </div>
              </Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
        <div className="mt-4 flex items-center justify-between gap-2 text-sm text-muted">
          <span>共 {listing.data?.total ?? 0} 项 · 每页 100 项</span>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="secondary"
              isDisabled={busy || page === 0}
              onPress={() => navigate(path, search, page - 1)}
            >
              上一页
            </Button>
            <span>第 {page + 1} 页</span>
            <Button
              size="sm"
              variant="secondary"
              isDisabled={
                busy || !listing.data || (page + 1) * 100 >= listing.data.total
              }
              onPress={() => navigate(path, search, page + 1)}
            >
              下一页
            </Button>
          </div>
        </div>
        <p className="mt-3 text-xs text-muted">
          单文件上传上限 500 MiB，同名文件不会覆盖。支持 UTF-8
          文本预览；符号链接和特殊文件仅展示。删除目录前需先清空目录内容。
        </p>
      </DataPanel>
      {renaming ? (
        <ModalShell
          title="重命名"
          onClose={() => {
            if (!busy) {
              setRenaming(null);
            }
          }}
        >
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await operationCommand(`${base}/mutate`, {
                  action: "rename",
                  path: renaming.path,
                  name,
                  expectedRevision: renaming.revision,
                });
                setRenaming(null);
                setSizes({});
                listing.refresh();
                toast.success("重命名完成");
              });
            }}
          >
            <Input
              aria-label="新名称"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              required
            />
            <Button
              type="submit"
              isDisabled={busy || !name.trim()}
              isPending={busy}
            >
              保存
            </Button>
          </form>
        </ModalShell>
      ) : null}
      {properties ? (
        <ModalShell
          title={`属性 · ${properties.name}`}
          onClose={() => setProperties(null)}
        >
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-5 gap-y-3 text-sm">
            {[
              ["名称", properties.name],
              ["路径", properties.absolutePath],
              ["类型", properties.kind === "directory" ? "目录" : "文件"],
              [
                "大小",
                properties.kind === "directory"
                  ? "按需计算，请使用表格中的计算按钮"
                  : sizeText(properties.size),
              ],
              ["权限", properties.mode],
              ["用户 / 用户组 ID", properties.owner],
              [
                "修改时间",
                properties.modifiedAt
                  ? new Date(properties.modifiedAt).toLocaleString("zh-CN")
                  : "—",
              ],
              [
                "创建时间",
                properties.createdAt
                  ? new Date(properties.createdAt).toLocaleString("zh-CN")
                  : "当前环境不提供",
              ],
            ].map(([label, value]) => (
              <div key={label} className="contents">
                <dt className="text-muted">{label}</dt>
                <dd className="break-all select-text">{value}</dd>
              </div>
            ))}
          </dl>
        </ModalShell>
      ) : null}
      {jumpOpen ? (
        <ModalShell title="跳转路径" onClose={() => setJumpOpen(false)}>
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              if (busy || !address?.trim()) return;
              navigate(address.trim(), "");
              setJumpOpen(false);
            }}
          >
            <Input
              aria-label="目标路径"
              placeholder="输入目录路径，例如 /var/log"
              value={address ?? path}
              onChange={(e) => setAddress(e.target.value)}
              autoFocus
              required
            />
            <p className="text-sm text-muted">
              路径相对于当前环境的管理根目录。
            </p>
            <Button type="submit" isDisabled={busy || !address?.trim()}>
              跳转
            </Button>
          </form>
        </ModalShell>
      ) : null}
      {create ? (
        <ModalShell
          title={create === "mkdir" ? "新建目录" : "新建文件"}
          onClose={() => {
            if (!busy) {
              setCreate(null);
            }
          }}
        >
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              void run(async () => {
                await operationCommand(`${base}/mutate`, {
                  action: create,
                  path,
                  name,
                });
                setCreate(null);
                setSizes({});
                listing.refresh();
                toast.success("创建完成");
              });
            }}
          >
            <Input
              aria-label="名称"
              placeholder="名称"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
            />
            <Button
              type="submit"
              isDisabled={busy || !name.trim()}
              isPending={busy}
            >
              创建
            </Button>
          </form>
        </ModalShell>
      ) : null}
      {deleting ? (
        <ModalShell
          title="删除文件或目录"
          onClose={() => {
            if (!busy) {
              setDeleting(null);
            }
          }}
        >
          <p className="mb-4 break-all">
            确认永久删除 {deleting.path}？此操作不可撤销。非空目录不会删除。
          </p>
          <Button
            variant="danger"
            isDisabled={busy}
            isPending={busy}
            onPress={() =>
              void run(async () => {
                await operationCommand(`${base}/mutate`, {
                  action: "delete",
                  path: deleting.path,
                  expectedRevision: deleting.revision,
                });
                setDeleting(null);
                setSizes({});
                listing.refresh();
                toast.success("删除完成");
              })
            }
          >
            确认删除
          </Button>
        </ModalShell>
      ) : null}
      {preview ? (
        <ModalShell
          title={`文件预览 · ${preview.name}`}
          width={1000}
          onClose={() => setPreview(null)}
        >
          <pre className="max-h-[65vh] overflow-auto whitespace-pre-wrap break-words rounded-lg bg-default p-4 text-sm">
            {preview.text}
          </pre>
        </ModalShell>
      ) : null}
    </>
  );
}
