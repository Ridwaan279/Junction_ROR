import { isLiveStatus } from "@/lib/live-data";
export const dynamic = "force-dynamic";
/** Provider credentials remain in Rust; the browser uses one origin. */
export async function GET() {
  const base = process.env.EMBERWATCH_API_BASE_URL || process.env.NEXT_PUBLIC_API_BASE_URL || "http://127.0.0.1:8080";
  try {
    const response = await fetch(base.replace(/\/$/, "") + "/api/live/status", {
      cache: "no-store", signal: AbortSignal.timeout(55_000), redirect: "error",
    });
    if (!response.ok) throw new Error("Upstream unavailable");
    const value: unknown = await response.json();
    if (!isLiveStatus(value)) throw new Error("Unexpected upstream response");
    return Response.json(value, { headers: { "Cache-Control": "no-store" } });
  } catch {
    return Response.json({ error: "LIVE_SERVICE_UNAVAILABLE" }, { status: 503, headers: { "Cache-Control": "no-store" } });
  }
}
