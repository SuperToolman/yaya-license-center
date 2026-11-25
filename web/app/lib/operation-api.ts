import type { Envelope } from "./operation-types";

const apiBaseUrl =
  process.env.NEXT_PUBLIC_OPERATION_API_BASE_URL ??
  process.env.NEXT_PUBLIC_LICENSE_API_BASE_URL ??
  (typeof window === "undefined"
    ? "http://127.0.0.1:8779"
    : `${window.location.protocol}//${window.location.hostname}:8779`);

export function operationUrl(path: string) {
  return `${apiBaseUrl}${path}`;
}

export async function operationRequest<T>(path: string, options?: RequestInit) {
  const response = await fetch(`${apiBaseUrl}${path}`, {
    ...options,
    credentials: "include",
  });
  let payload: Envelope<T>;
  try {
    const value: unknown = await response.json();
    if (!value || typeof value !== "object" || !("data" in value)) {
      throw new Error("invalid envelope");
    }
    payload = value as Envelope<T>;
  } catch {
    throw new Error(
      `运营服务返回了无效响应（HTTP ${response.status}），请稍后重试。`,
    );
  }
  return { response, payload };
}

export async function operationData<T>(
  path: string,
  options?: RequestInit,
): Promise<T> {
  const { response, payload } = await operationRequest<T>(path, options);
  if (!response.ok || payload.data === null || payload.data === undefined) {
    throw new Error(
      payload.message || `数据读取失败（HTTP ${response.status}）`,
    );
  }
  return payload.data;
}

export async function operationCommand<T = unknown>(
  path: string,
  body?: unknown,
  method = "POST",
) {
  const { response, payload } = await operationJson<T>(path, body, method);
  if (!response.ok)
    throw new Error(payload.message || `操作失败（HTTP ${response.status}）`);
  return payload.data;
}

export function operationError(cause: unknown) {
  return cause instanceof TypeError
    ? "无法连接运营管理服务，请检查网络后重试。"
    : cause instanceof Error
      ? cause.message
      : "操作失败，请稍后重试。";
}

export async function operationJson<T>(
  path: string,
  body?: unknown,
  method = "POST",
) {
  return operationRequest<T>(path, {
    method,
    headers:
      body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}
