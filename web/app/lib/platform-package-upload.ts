import { operationUrl } from "./operation-api";

export function uploadPlatformPackage<T>(
  file: File,
  onProgress: (loaded: number, total: number, validating: boolean) => void,
  signal: AbortSignal,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const original = operationUrl("/api/platform-versions");
    // Same-origin deployments bypass Next's rewrite body cloning and 30s proxy timeout.
    const url = original.startsWith("/operation-api/")
      ? "/api/platform-package-upload"
      : original;
    xhr.open("POST", url);
    xhr.withCredentials = true;
    xhr.timeout = 60 * 60 * 1000;
    const abort = () => xhr.abort();
    const clean = () => signal.removeEventListener("abort", abort);
    signal.addEventListener("abort", abort, { once: true });
    xhr.upload.onprogress = (event) => {
      const ratio =
        event.lengthComputable && event.total ? event.loaded / event.total : 0;
      onProgress(
        Math.min(file.size, Math.round(file.size * ratio)),
        file.size,
        false,
      );
    };
    xhr.upload.onload = () => onProgress(file.size, file.size, true);
    xhr.onload = () => {
      clean();
      let payload: { message?: string; data?: T };
      try {
        payload = JSON.parse(xhr.responseText);
      } catch {
        reject(
          new Error(
            xhr.status === 413
              ? "上传请求超过服务器限制（HTTP 413）。"
              : `上传通道返回异常响应（HTTP ${xhr.status}），请检查服务器上传服务日志。`,
          ),
        );
        return;
      }
      if (xhr.status < 200 || xhr.status >= 300 || !payload?.data) {
        reject(
          new Error(payload?.message || `平台包上传失败（HTTP ${xhr.status}）`),
        );
      } else resolve(payload.data);
    };
    xhr.onerror = () => {
      clean();
      reject(new Error("上传连接中断，请检查网络后重试。"));
    };
    xhr.ontimeout = () => {
      clean();
      reject(
        new Error("上传或服务器校验超时；重试前请刷新列表确认是否已经入库。"),
      );
    };
    xhr.onabort = () => {
      clean();
      reject(new DOMException("上传已取消", "AbortError"));
    };
    if (signal.aborted) {
      clean();
      reject(new DOMException("上传已取消", "AbortError"));
      return;
    }
    const form = new FormData();
    form.append("file", file);
    xhr.send(form);
  });
}
