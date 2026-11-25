"use client";

import { Button, Modal, ProgressBar } from "@heroui/react";
import { useEffect, useRef, useState } from "react";
import { uploadPlatformPackage } from "../../lib/platform-package-upload";

type UploadedPackage = {
  version: string;
  packageId: string;
  size: number;
  filename: string;
};
const size = (value: number) => `${(value / 1024 / 1024).toFixed(1)} MiB`;
const duration = (seconds: number) =>
  seconds < 60
    ? `${Math.floor(seconds)} 秒`
    : `${Math.floor(seconds / 60)} 分 ${Math.floor(seconds % 60)} 秒`;

export function PackageUploadModal({
  file,
  onSuccess,
  onClose,
}: {
  file: File;
  onSuccess: () => void;
  onClose: () => void;
}) {
  const [phase, setPhase] = useState<
    "uploading" | "validating" | "succeeded" | "failed" | "cancelled"
  >("uploading");
  const [loaded, setLoaded] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [transferSeconds, setTransferSeconds] = useState(0);
  const [message, setMessage] = useState("");
  const [result, setResult] = useState<UploadedPackage | null>(null);
  const [attempt, setAttempt] = useState(0);
  const controller = useRef<AbortController | null>(null);
  const success = useRef(onSuccess);
  useEffect(() => {
    success.current = onSuccess;
  }, [onSuccess]);
  const busy = phase === "uploading" || phase === "validating";

  useEffect(() => {
    const abort = new AbortController();
    controller.current = abort;
    const started = Date.now();
    let stopped = false;
    const timer = setInterval(() => {
      if (!stopped) setElapsed((Date.now() - started) / 1000);
    }, 500);
    // Defer to avoid duplicate POSTs during React development effect replay.
    const launch = setTimeout(() => {
      const run = async () => {
        if (!file.name.toLowerCase().endsWith(".zip"))
          throw new Error("请选择离线镜像 ZIP 包。");
        if (!file.size || file.size > 8 * 1024 ** 3)
          throw new Error("平台包必须非空且不超过 8 GiB。");
        return uploadPlatformPackage<UploadedPackage>(
          file,
          (bytes, _total, validating) => {
            if (abort.signal.aborted) return;
            setLoaded(bytes);
            if (validating) {
              setPhase("validating");
              setTransferSeconds((Date.now() - started) / 1000);
            }
          },
          abort.signal,
        );
      };
      void run()
        .then((value) => {
          if (abort.signal.aborted) return;
          setResult(value);
          setPhase("succeeded");
          success.current();
        })
        .catch((error) => {
          if (abort.signal.aborted) return;
          setMessage(error instanceof Error ? error.message : "上传失败");
          setPhase("failed");
        })
        .finally(() => {
          stopped = true;
          clearInterval(timer);
          if (!abort.signal.aborted) setElapsed((Date.now() - started) / 1000);
        });
    }, 0);
    return () => {
      clearTimeout(launch);
      clearInterval(timer);
      abort.abort();
    };
  }, [file, attempt]);

  const speed =
    loaded / Math.max(phase === "uploading" ? elapsed : transferSeconds, 0.1);
  const percent = file.size ? Math.min(100, (loaded / file.size) * 100) : 0;
  const title =
    phase === "uploading"
      ? "正在上传"
      : phase === "validating"
        ? "服务器正在校验"
        : phase === "succeeded"
          ? "上传成功"
          : phase === "cancelled"
            ? "上传已取消"
            : "上传失败";
  return (
    <Modal
      isOpen
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <Modal.Backdrop isDismissable={!busy}>
        <Modal.Container placement="center" scroll="inside">
          <Modal.Dialog
            style={{
              width: "min(640px, calc(100vw - 2rem))",
              maxWidth: "none",
            }}
          >
            <Modal.Header>
              <Modal.Heading>上传平台版本包</Modal.Heading>
              <Modal.CloseTrigger aria-label="关闭" isDisabled={busy} />
            </Modal.Header>
            <Modal.Body className="space-y-4">
              <p className="break-all text-sm font-medium">{file.name}</p>
              <p
                role="status"
                className={
                  phase === "failed"
                    ? "text-danger"
                    : phase === "succeeded"
                      ? "text-success"
                      : "text-muted"
                }
              >
                {title}
                {phase === "uploading" ? ` · ${percent.toFixed(1)}%` : ""}
              </p>
              <ProgressBar aria-label="平台包上传进度" value={percent}>
                <ProgressBar.Track>
                  <ProgressBar.Fill />
                </ProgressBar.Track>
              </ProgressBar>
              <dl className="grid grid-cols-2 gap-3 text-sm">
                <div>
                  <dt className="text-muted">传输大小</dt>
                  <dd>
                    {size(loaded)} / {size(file.size)}
                  </dd>
                </div>
                <div>
                  <dt className="text-muted">已耗时</dt>
                  <dd>{duration(elapsed)}</dd>
                </div>
                <div>
                  <dt className="text-muted">平均上传速度</dt>
                  <dd>
                    {loaded && elapsed > 0 ? `${size(speed)}/秒` : "计算中"}
                  </dd>
                </div>
                <div>
                  <dt className="text-muted">预计剩余时间</dt>
                  <dd>
                    {phase === "uploading"
                      ? loaded && elapsed >= 1
                        ? `约 ${duration((file.size - loaded) / speed)}`
                        : "计算中"
                      : phase === "validating"
                        ? "校验时长暂无法预估"
                        : "—"}
                  </dd>
                </div>
              </dl>
              {phase === "validating" ? (
                <p className="text-sm text-muted">
                  文件传输完成，正在验证 ZIP、镜像归档与 SHA-256 并保存。已等待{" "}
                  {duration(Math.max(0, elapsed - transferSeconds))}
                  ，收到服务器结果后才算上传成功。
                </p>
              ) : null}
              {message ? (
                <p role="alert" className="break-words text-sm text-danger">
                  {message}
                </p>
              ) : null}
              {result ? (
                <div className="space-y-2 text-sm">
                  <p>平台版本 {result.version} 上传成功</p>
                  <p className="break-all font-mono text-xs">
                    包标识：{result.packageId}
                  </p>
                  <p>平台版本列表已更新，可用于客户实例部署。</p>
                </div>
              ) : null}
              {phase === "cancelled" ? (
                <p className="text-sm text-muted">
                  已中断本次连接。如果服务器已进入校验阶段，请先刷新列表确认入库结果再重试。
                </p>
              ) : null}
            </Modal.Body>
            <Modal.Footer>
              {busy ? (
                <Button
                  variant="danger-soft"
                  onPress={() => {
                    controller.current?.abort();
                    setPhase("cancelled");
                  }}
                >
                  取消上传
                </Button>
              ) : (
                <>
                  <Button variant="secondary" onPress={onClose}>
                    关闭
                  </Button>
                  {phase !== "succeeded" ? (
                    <Button
                      onPress={() => {
                        setLoaded(0);
                        setElapsed(0);
                        setTransferSeconds(0);
                        setMessage("");
                        setPhase("uploading");
                        setAttempt((value) => value + 1);
                      }}
                    >
                      重新上传
                    </Button>
                  ) : null}
                </>
              )}
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}
