import type { Envelope } from "./operation-types";

const apiBaseUrl = process.env.NEXT_PUBLIC_OPERATION_API_BASE_URL ?? process.env.NEXT_PUBLIC_LICENSE_API_BASE_URL ?? (typeof window === "undefined" ? "http://127.0.0.1:8779" : `${window.location.protocol}//${window.location.hostname}:8779`);

export async function operationRequest<T>(path: string, options?: RequestInit) {
  const response = await fetch(`${apiBaseUrl}${path}`, { ...options, credentials: "include" });
  const payload = (await response.json()) as Envelope<T>;
  return { response, payload };
}

export async function operationJson<T>(path: string, body?: unknown, method = "POST") {
  return operationRequest<T>(path, { method, headers: body === undefined ? undefined : { "content-type": "application/json" }, body: body === undefined ? undefined : JSON.stringify(body) });
}
