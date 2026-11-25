"use client";
import { createClientId } from "../../../lib/client-id";
import { useEffect, useRef, useState } from "react";
import { useOperation } from "../../../components/OperationProvider";
import { Button, Card, Modal, Spinner } from "@heroui/react";
import { Plus, TrashBin } from "@gravity-ui/icons";
import {
  operationData,
  operationCommand,
  operationError,
} from "../../../lib/operation-api";
type Manifest = {
  version: string;
  documents: { id: string; name: string; size: number; sha256: string }[];
};
export function EmployeeKnowledge({
  employeeId,
  disabled,
}: {
  employeeId?: string;
  disabled: boolean;
}) {
  const { refresh } = useOperation();
  const [data, setData] = useState<Manifest>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [deleting, setDeleting] = useState<string>();
  const input = useRef<HTMLInputElement>(null);
  const endpoint = `/api/ai-employees/${encodeURIComponent(employeeId ?? "")}/knowledge`;
  useEffect(() => {
    if (!employeeId) return;
    const c = new AbortController();
    void operationData<Manifest>(endpoint, { signal: c.signal })
      .then(setData)
      .catch((e) => {
        if (!c.signal.aborted) setError(operationError(e));
      });
    return () => c.abort();
  }, [employeeId, endpoint]);
  async function upload(file: File) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const query = new URLSearchParams({
        name: file.name,
        clientRequestId: createClientId(),
      });
      setData(
        await operationData<Manifest>(`${endpoint}?${query}`, {
          method: "POST",
          headers: { "content-type": "application/octet-stream" },
          body: file,
        }),
      );
      refresh();
    } catch (e) {
      setError(operationError(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    if (!deleting) return;
    setBusy(true);
    setError("");
    try {
      setData(
        (await operationCommand<Manifest>(
          `${endpoint}/${encodeURIComponent(deleting)}`,
          undefined,
          "DELETE",
        )) ?? undefined,
      );
      setDeleting(undefined);
      refresh();
    } catch (e) {
      setError(operationError(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="flex flex-col gap-3" aria-label="员工专属知识库">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3>员工专属知识库</h3>
          <p className="text-sm text-muted">
            原件随员工版本发布。客户更新安装后同步；保存原件不代表已建立检索索引。
          </p>
        </div>
        <Button
          type="button"
          variant="secondary"
          isDisabled={disabled || busy || !employeeId}
          onPress={() => input.current?.click()}
        >
          <Plus />
          上传文档
        </Button>
      </div>
      <input
        ref={input}
        type="file"
        hidden
        accept=".txt,.md,.csv,.tsv,.pdf,.docx,.xlsx,.ods,.pptx"
        onChange={(e) => {
          const f = e.target.files?.[0];
          e.target.value = "";
          if (f) void upload(f);
        }}
      />
      {!employeeId ? (
        <p className="text-sm text-muted">请先保存员工，再上传知识文档。</p>
      ) : (
        <p className="text-xs text-muted">
          员工版本：{data?.version ?? "—"} · 最多 20 份 / 40 MiB，单文件 8
          MiB。增删文档会立即更新员工版本，独立于下方基础信息保存。
        </p>
      )}
      {busy && <Spinner size="sm" aria-label="处理知识文档" />}
      {error && (
        <p role="alert" className="text-danger">
          {error}
        </p>
      )}
      {data?.documents.map((d) => (
        <Card key={d.id} variant="secondary">
          <Card.Content className="flex flex-row items-center gap-3">
            <span className="min-w-0 flex-1 truncate">{d.name}</span>
            <span className="text-xs text-muted">
              {(d.size / 1024).toFixed(1)} KiB · 已保存
            </span>
            <Button
              type="button"
              isIconOnly
              variant="ghost"
              isDisabled={busy || disabled}
              aria-label={`移除 ${d.name}`}
              onPress={() => setDeleting(d.id)}
            >
              <TrashBin />
            </Button>
          </Card.Content>
        </Card>
      ))}
      {employeeId && data && !data.documents.length && (
        <p className="text-sm text-muted">暂无知识文档。</p>
      )}
      <Modal
        isOpen={Boolean(deleting)}
        onOpenChange={(open) => {
          if (!open && !busy) setDeleting(undefined);
        }}
      >
        <Modal.Backdrop isDismissable={!busy}>
          <Modal.Container size="sm">
            <Modal.Dialog>
              <Modal.Header>
                <Modal.Heading>移除员工知识文档</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                文档将从后续员工包中移除，已安装客户需更新员工。不会修改客户自行维护的知识。
              </Modal.Body>
              <Modal.Footer>
                <Button
                  type="button"
                  variant="ghost"
                  isDisabled={busy}
                  onPress={() => setDeleting(undefined)}
                >
                  取消
                </Button>
                <Button
                  type="button"
                  variant="danger"
                  isPending={busy}
                  onPress={() => void remove()}
                >
                  确认移除
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      </Modal>
    </section>
  );
}
