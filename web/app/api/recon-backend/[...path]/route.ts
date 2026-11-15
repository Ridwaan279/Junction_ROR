import type { NextRequest } from "next/server";

// Server-only origin; never forwards arbitrary URLs, cookies, or credentials.
async function proxy(request: NextRequest, context: { params: Promise<{ path: string[] }> }) {
  const { path } = await context.params;
  const pathname = "/" + path.join("/");
  if (!/^\/api\/(snapshot|recon\/tasks(?:\/[a-f0-9-]+\/(?:return|frame))?)$/.test(pathname)) {
    return Response.json({ error: "Unsupported recon path" }, { status: 404 });
  }
  const base = process.env.RECON_API_BASE_URL || "http://127.0.0.1:8080";
  try {
    const upstream = await fetch(base + pathname + request.nextUrl.search, {
      method: request.method,
      headers: request.method === "POST" ? { "Content-Type": "application/json" } : {},
      body: request.method === "POST" ? await request.text() : undefined,
      cache: "no-store",
      signal: AbortSignal.timeout(5000),
    });
    return new Response(upstream.body, {
      status: upstream.status,
      headers: { "Content-Type": upstream.headers.get("content-type") || "application/json", "Cache-Control": "no-store" },
    });
  } catch {
    return Response.json({ error: "Recon backend unavailable" }, { status: 502 });
  }
}
export const GET = proxy;
export const POST = proxy;
