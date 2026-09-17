import { NextResponse } from "next/server";

const lowCodeBaseUrl =
  process.env.LOW_CODE_API_BASE_URL ??
  process.env.NEXT_PUBLIC_LOW_CODE_API_BASE_URL ??
  "http://127.0.0.1:8788";

export async function GET(request: Request) {
  try {
    const response = await fetch(`${lowCodeBaseUrl}/api/apps`, {
      headers: {
        cookie: request.headers.get("cookie") ?? "",
        authorization: request.headers.get("authorization") ?? "",
      },
      cache: "no-store",
    });
    const body = await response.text();
    return new NextResponse(body, {
      status: response.status,
      headers: { "content-type": response.headers.get("content-type") ?? "application/json" },
    });
  } catch {
    return NextResponse.json(
      { code: 503, message: "低代码平台暂不可用", data: null },
      { status: 503 },
    );
  }
}
