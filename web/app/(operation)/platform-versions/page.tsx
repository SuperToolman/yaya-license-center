"use client";

import { Button, Input, Table } from "@heroui/react";
import { useRef, useState } from "react";
import OperationPageHeader from "../../components/OperationPageHeader";
import { useOperation } from "../../components/OperationProvider";
import { DataPanel } from "../../components/ui/DataPanel";
import { ModalShell } from "../../components/ui/ModalShell";
import { DataTable } from "../../components/ui/DataTable";
import { useOperationData } from "../../hooks/useOperationData";
import { operationUrl } from "../../lib/operation-api";
import { useErrorNotification } from "../../hooks/useErrorNotification";
import { PackageUploadModal } from "./PackageUploadModal";

type PlatformPackage = {
  packageId: string;
  version: string;
  filename: string;
  artifactType: string;
  builtAt: string;
  uploadedAt: number;
  uploadedBy: string;
  uploadedByName: string;
  size: number;
  sha256: string;
  fileCount: number;
  imageId?: string | null;
  imagePlatform?: string | null;
};

export default function PlatformVersionsPage() {
  const { currentUser } = useOperation();
  if (!currentUser) return <p className="text-muted">正在读取账号权限…</p>;
  if (currentUser.role !== "platform_admin")
    return <p role="alert">仅平台超级管理员可访问平台版本包。</p>;
  return <PlatformVersions />;
}

function PlatformVersions() {
  const { data, error, loading, refresh } = useOperationData<PlatformPackage[]>(
    "/api/platform-versions",
  );
  const [detail, setDetail] = useState<PlatformPackage | null>(null);
  const [search, setSearch] = useState("");
  const [file, setFile] = useState<File | null>(null);
  useErrorNotification(error, loading);
  const input = useRef<HTMLInputElement>(null);
  const items = (data ?? []).filter((item) =>
    `${item.version} ${item.packageId} ${item.filename}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  return (
    <>
      <OperationPageHeader
        eyebrow="PLATFORM VERSIONS"
        title="平台版本管理"
        description="管理低代码平台部署包，为客户平台实例与上线准备版本。"
        loading={loading}
        onRefresh={refresh}
        actions={
          <Button isDisabled={!!file} onPress={() => input.current?.click()}>
            上传平台包
          </Button>
        }
      />
      <input
        ref={input}
        type="file"
        accept=".zip,application/zip"
        aria-label="选择平台版本包"
        hidden
        disabled={!!file}
        onChange={(event) => {
          const selected = event.target.files?.[0];
          event.target.value = "";
          if (selected) setFile(selected);
        }}
      />
      {file ? (
        <PackageUploadModal
          file={file}
          onSuccess={refresh}
          onClose={() => setFile(null)}
        />
      ) : null}
      <DataPanel
        title="平台版本包"
        note={`${data?.length ?? 0} 个离线镜像包；同一版本可保存多个构建，点击详情查看包标识与完整校验值。`}
      >
        <div className="flex justify-end px-4 py-3">
          <Input
            aria-label="搜索平台版本"
            placeholder="搜索版本或 UUID"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            className="w-full sm:max-w-xs"
          />
        </div>
        <DataTable
          label="平台版本包"
          contentClassName="min-w-[1120px]"
          columns={[
            "版本",
            "包大小",
            "镜像架构",
            "上传人",
            "构建时间",
            "上传时间",
            "操作",
          ]}
          columnWidths={{
            版本: 100,
            包大小: 120,
            镜像架构: 140,
            上传人: 140,
            构建时间: 190,
            上传时间: 190,
            操作: 140,
          }}
          empty={
            loading
              ? "正在加载平台版本包…"
              : error
                ? "版本包读取失败，请刷新重试。"
                : search
                  ? "没有匹配的平台包。"
                  : "暂无平台版本包，请先上传打包脚本生成的 ZIP 文件。"
          }
        >
          {items.map((item) => (
            <Table.Row key={item.packageId} id={item.packageId}>
              <Table.Cell>
                <strong>{item.version}</strong>
              </Table.Cell>
              <Table.Cell className="whitespace-nowrap">
                {(item.size / 1024 / 1024).toFixed(2)} MiB
              </Table.Cell>
              <Table.Cell className="whitespace-nowrap">
                {item.imagePlatform ?? "—"}
              </Table.Cell>
              <Table.Cell>{item.uploadedByName || "未知用户"}</Table.Cell>
              <Table.Cell className="whitespace-nowrap">
                {new Date(item.builtAt).toLocaleString()}
              </Table.Cell>
              <Table.Cell className="whitespace-nowrap">
                {new Date(item.uploadedAt).toLocaleString()}
              </Table.Cell>
              <Table.Cell>
                <div className="flex items-center gap-3 whitespace-nowrap">
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`详情 ${item.version} ${item.packageId}`}
                    onPress={() => setDetail(item)}
                  >
                    详情
                  </Button>
                  <a
                    className="text-accent hover:underline"
                    href={operationUrl(
                      `/api/platform-versions/${encodeURIComponent(item.packageId)}/download`,
                    )}
                    aria-label={`下载 ${item.version} ${item.packageId}`}
                  >
                    下载
                  </a>
                </div>
              </Table.Cell>
            </Table.Row>
          ))}
        </DataTable>
      </DataPanel>
      {detail ? (
        <ModalShell
          title="平台包详情"
          width={720}
          onClose={() => setDetail(null)}
        >
          <dl className="space-y-4 text-sm">
            {[
              ["版本", detail.version],
              ["包标识 UUID", detail.packageId],
              ["文件名", detail.filename],
              ["包类型", "离线镜像包"],
              ["包大小", `${(detail.size / 1024 / 1024).toFixed(2)} MiB`],
              ["文件数量", String(detail.fileCount)],
              ["镜像架构", detail.imagePlatform ?? "—"],
              ["上传人", detail.uploadedByName || "未知用户"],
              ["构建时间", new Date(detail.builtAt).toLocaleString()],
              ["上传时间", new Date(detail.uploadedAt).toLocaleString()],
            ].map(([label, value]) => (
              <div
                key={label}
                className="grid grid-cols-[100px_minmax(0,1fr)] gap-3"
              >
                <dt className="text-muted">{label}</dt>
                <dd className="break-all">{value}</dd>
              </div>
            ))}
            <div>
              <dt className="mb-1 text-muted">Docker 镜像标识（内容摘要）</dt>
              <dd>
                <code className="block break-all text-xs">
                  {detail.imageId ?? "—"}
                </code>
              </dd>
              <dd className="mt-1 text-xs text-muted">
                用于识别导入的镜像，与 ZIP 文件校验值的计算对象不同。
              </dd>
            </div>
            <div>
              <dt className="mb-1 text-muted">ZIP 文件 SHA-256</dt>
              <dd>
                <code className="block break-all text-xs">{detail.sha256}</code>
              </dd>
              <dd className="mt-1 text-xs text-muted">
                对整个上传 ZIP
                文件计算，用于验证文件完整性；无需与镜像标识相同。
              </dd>
            </div>
          </dl>
        </ModalShell>
      ) : null}
    </>
  );
}
