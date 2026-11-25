import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";
const limit = 8 * 1024 ** 3 + 1024 ** 2;
const failure = (status: number, message: string) =>
  Response.json({ data: null, message }, { status });

// A dedicated route avoids Next rewrite's cloning buffer (512 MiB) and its
// default 30-second upstream timeout. Backpressure keeps memory bounded.
export async function POST(request: Request) {
  if (
    !request.headers
      .get("content-type")
      ?.toLowerCase()
      .startsWith("multipart/form-data;")
  )
    return failure(400, "请使用文件上传请求");
  if (Number(request.headers.get("content-length")) > limit)
    return failure(413, "平台包不得超过 8 GiB");
  if (!request.body) return failure(400, "请选择平台 ZIP 包");
  const target = new URL(
    "/api/platform-versions",
    process.env.YAYA_OPERATION_API_UPSTREAM || "http://127.0.0.1:8779",
  );
  const headers: Record<string, string> = {};
  for (const name of [
    "content-type",
    "content-length",
    "cookie",
    "authorization",
    "origin",
    "sec-fetch-site",
  ]) {
    const value = request.headers.get(name);
    if (value) headers[name] = value;
  }
  return new Promise<Response>((resolve) => {
    let settled = false;
    let tooLarge = false;
    const finish = (response: Response) => {
      if (!settled) {
        settled = true;
        request.signal.removeEventListener("abort", abort);
        resolve(response);
      }
    };
    const upstream = (
      target.protocol === "https:" ? httpsRequest : httpRequest
    )(target, { method: "POST", headers }, (response) => {
      const chunks: Buffer[] = [];
      let bytes = 0;
      response.on("data", (chunk: Buffer) => {
        bytes += chunk.length;
        if (bytes > 1024 * 1024)
          response.destroy(new Error("upstream response too large"));
        else chunks.push(chunk);
      });
      response.on("error", () =>
        finish(failure(502, "上传服务响应中断；请刷新列表确认是否已入库。")),
      );
      response.on("end", () => {
        let value: unknown;
        try {
          value = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        } catch {
          finish(
            failure(
              502,
              `上传服务返回无效响应（HTTP ${response.statusCode}），请检查 API 日志。`,
            ),
          );
          return;
        }
        finish(
          Response.json(value, {
            status: response.statusCode || 502,
            headers: { "cache-control": "no-store" },
          }),
        );
      });
    });
    const abort = () => upstream.destroy(new Error("client disconnected"));
    request.signal.addEventListener("abort", abort, { once: true });
    if (request.signal.aborted) abort();
    // This is an inactivity timeout, not a total upload duration limit.
    upstream.setTimeout(30 * 60 * 1000, () =>
      upstream.destroy(new Error("upload idle timeout")),
    );
    upstream.on("error", (error) => {
      console.error("Platform package upload proxy:", error.message);
      finish(
        failure(
          tooLarge ? 413 : 502,
          tooLarge
            ? "平台包不得超过 8 GiB"
            : "上传服务连接中断，请检查网络及 API 状态；重试前请刷新列表确认入库结果。",
        ),
      );
    });
    let received = 0;
    const bounded = new Transform({
      transform(chunk, _encoding, callback) {
        received += chunk.length;
        if (received > limit) {
          tooLarge = true;
          callback(new Error("upload body limit exceeded"));
        } else callback(null, chunk);
      },
    });
    void pipeline(
      Readable.fromWeb(
        request.body as import("node:stream/web").ReadableStream,
      ),
      bounded,
      upstream,
    ).catch(() => {
      // upstream error and early-response handlers produce the public envelope.
    });
  });
}
