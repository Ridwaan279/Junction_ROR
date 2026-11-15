export type LiveSource = {
  name: string; url: string; via: string; request_id: string | null;
  captured_at: string | null; last_successful_capture_at?: string | null;
  status: string; is_partial: boolean; credits: number | null;
  capture_succeeded: boolean; positively_empty: boolean; error_code: string | null;
};
export type LiveStatus = {
  mode: "live"; origin: "service"; checked_at: string;
  state: "no_active_fires" | "active_fires" | "check_failed" | "partial";
  statement: string | null; statement_en: string | null;
  active: { id: string; place: string; position: [number, number] | null; status: string; url: string }[];
  sources: LiveSource[]; note?: string;
};
/** A recorded fixture is never promoted to a current service result. */
export function isLiveStatus(value: unknown): value is LiveStatus {
  if (!value || typeof value !== "object") return false;
  const v = value as Record<string, unknown>;
  return v.mode === "live" && v.origin === "service"
    && typeof v.checked_at === "string" && Number.isFinite(Date.parse(v.checked_at))
    && ["no_active_fires", "active_fires", "check_failed", "partial"].includes(String(v.state))
    && Array.isArray(v.active) && v.active.every((incident: unknown) => {
      if (!incident || typeof incident !== "object") return false;
      const row = incident as LiveStatus["active"][number];
      return [row.id,row.place,row.status,row.url].every((s) => typeof s === "string")
        && (row.position === null || Array.isArray(row.position) && row.position.length === 2 && row.position.every(Number.isFinite));
    }) && Array.isArray(v.sources)
    && v.sources.every((source: unknown) => Boolean(source && typeof source === "object"
      && typeof (source as LiveSource).name === "string" && typeof (source as LiveSource).url === "string"));
}
export const LIVE_REFRESH_MS = 60_000;
