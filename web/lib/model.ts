import type { EventEnvelope, Snapshot } from "./contracts";
export interface StreamState {
  snapshot: Snapshot;
  needsResync: boolean;
  seen: string[];
}
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
const timestamp = (value: unknown): value is string =>
  typeof value === "string" && Number.isFinite(Date.parse(value));
const position = (value: unknown) =>
  value === null ||
  (Array.isArray(value) &&
    value.length === 2 &&
    value.every((v) => typeof v === "number" && Number.isFinite(v)) &&
    Math.abs(value[0]) <= 180 &&
    Math.abs(value[1]) <= 90);
export function isSnapshot(value: unknown): value is Snapshot {
  if (!record(value) || !record(value.scenario)) return false;
  const scenario = value.scenario;
  if (
    typeof scenario.id !== "string" ||
    typeof scenario.run_id !== "string" ||
    typeof scenario.name !== "string" ||
    !["live", "historical", "synthetic"].includes(String(scenario.mode)) ||
    !timestamp(scenario.current_at) ||
    !timestamp(scenario.start_at) ||
    !timestamp(scenario.end_at) ||
    !Number.isInteger(scenario.epoch) ||
    Number(scenario.epoch) < 0 ||
    typeof scenario.playing !== "boolean" ||
    typeof scenario.speed !== "number"
  )
    return false;
  const keys = [
    "incidents",
    "evidence",
    "weather",
    "sources",
    "forecasts",
    "drone_plans",
    "briefings",
  ];
  if (!keys.every((key) => Array.isArray(value[key]))) return false;
  for (const key of keys.filter((k) => k !== "sources")) {
    if (
      !(value[key] as unknown[]).every(
        (item) =>
          record(item) &&
          typeof item.id === "string" &&
          item.run_id === scenario.run_id,
      )
    )
      return false;
  }
  if (
    !(value.incidents as unknown[]).every(
      (i) =>
        record(i) &&
        typeof i.title === "string" &&
        Array.isArray(i.evidence_ids) &&
        Array.isArray(i.origin_groups) &&
        position(i.position) &&
        timestamp(i.updated_at),
    )
  )
    return false;
  if (
    !(value.evidence as unknown[]).every(
      (e) =>
        record(e) &&
        record(e.provenance) &&
        typeof e.title === "string" &&
        timestamp(e.provenance.available_at) &&
        timestamp(e.provenance.captured_at) &&
        typeof e.provenance.source_url === "string" &&
        Array.isArray(e.provenance.quality_flags) &&
        position(e.position),
    )
  )
    return false;
  if (
    !(value.sources as unknown[]).every(
      (s) =>
        record(s) &&
        typeof s.name === "string" &&
        typeof s.source_url === "string" &&
        record(s.coverage) &&
        Array.isArray(s.coverage.notes) &&
        Array.isArray(s.notes),
    )
  )
    return false;
  if (
    !(value.weather as unknown[]).every(
      (w) =>
        record(w) &&
        record(w.provenance) &&
        timestamp(w.valid_at) &&
        position(w.position),
    )
  )
    return false;
  if (
    !(value.forecasts as unknown[]).every(
      (f) =>
        record(f) &&
        f.epoch === scenario.epoch &&
        record(f.geometry) &&
        Array.isArray(f.limitations) &&
        timestamp(f.generated_at),
    )
  )
    return false;
  if (
    !(value.drone_plans as unknown[]).every(
      (d) =>
        record(d) &&
        d.epoch === scenario.epoch &&
        Array.isArray(d.route) &&
        d.route.every(position) &&
        Array.isArray(d.return_route) &&
        d.return_route.every(position) &&
        Array.isArray(d.reasons) &&
        timestamp(d.generated_at),
    )
  )
    return false;
  if (
    !(value.briefings as unknown[]).every(
      (b) =>
        record(b) &&
        b.epoch === scenario.epoch &&
        typeof b.text_pt === "string" &&
        typeof b.text_en === "string" &&
        Array.isArray(b.citations) &&
        timestamp(b.generated_at),
    )
  )
    return false;
  return true;
}
export function parseSnapshot(value: unknown): Snapshot {
  if (!isSnapshot(value)) throw new Error("Invalid or mixed-run snapshot");
  return value;
}
function cleared(snapshot: Snapshot, epoch: number, at: string): Snapshot {
  return {
    ...snapshot,
    scenario: { ...snapshot.scenario, epoch, current_at: at, playing: false },
    incidents: [],
    evidence: [],
    weather: [],
    forecasts: [],
    drone_plans: [],
    briefings: [],
  };
}
export function applyEvent(
  state: StreamState,
  event: EventEnvelope,
): StreamState {
  const current = state.snapshot.scenario;
  if (
    event.run_id !== current.run_id ||
    (event.scenario_id !== null && event.scenario_id !== current.id) ||
    event.epoch < current.epoch ||
    state.seen.includes(event.id)
  )
    return state;
  const seen = [...state.seen, event.id].slice(-512);
  if (
    isSnapshot(event.payload) &&
    event.payload.scenario.run_id === current.run_id &&
    event.payload.scenario.id === current.id &&
    event.payload.scenario.epoch === event.epoch
  ) {
    if (
      event.type === "scenario.tick" &&
      event.epoch === current.epoch &&
      Date.parse(event.payload.scenario.current_at) <
        Date.parse(current.current_at)
    )
      return state;
    return { snapshot: event.payload, needsResync: false, seen };
  }
  const snapshot =
    event.epoch > current.epoch || event.type === "run.reset"
      ? cleared(state.snapshot, event.epoch, event.event_time)
      : state.snapshot;
  return { snapshot, needsResync: true, seen };
}
export function replayAt(
  snapshot: Snapshot,
  at: string,
  epoch: number,
): Snapshot {
  const time = Math.min(
    Date.parse(snapshot.scenario.end_at),
    Math.max(Date.parse(snapshot.scenario.start_at), Date.parse(at)),
  );
  if (!Number.isFinite(time)) throw new Error("Invalid replay time");
  const evidence = snapshot.evidence.filter(
    (e) => Date.parse(e.provenance.available_at) <= time,
  );
  const available = new Set(evidence.map((e) => e.id));
  const generated = (item: { generated_at: string; epoch: number }) =>
    item.epoch === epoch && Date.parse(item.generated_at) <= time;
  return {
    ...snapshot,
    scenario: {
      ...snapshot.scenario,
      current_at: new Date(time).toISOString(),
      epoch,
    },
    evidence,
    incidents: snapshot.incidents.filter(
      (i) =>
        Date.parse(i.updated_at) <= time &&
        i.evidence_ids.every((id) => available.has(id)),
    ),
    weather: snapshot.weather.filter(
      (w) => Date.parse(w.provenance.available_at) <= time,
    ),
    forecasts: snapshot.forecasts.filter(generated),
    drone_plans: snapshot.drone_plans.filter(generated),
    briefings: snapshot.briefings.filter(generated),
  };
}
export function coverageMessage(
  snapshot: Snapshot,
): "unknown" | "partial" | "verified" {
  if (
    !snapshot.sources.length ||
    !snapshot.sources.some(
      (s) =>
        s.enabled &&
        ["healthy", "empty_verified", "partial"].includes(s.health),
    )
  )
    return "unknown";
  return snapshot.sources.every(
    (s) =>
      s.enabled &&
      s.coverage.complete &&
      ["healthy", "empty_verified"].includes(s.health),
  )
    ? "verified"
    : "partial";
}
export function safeUrl(value: string | null | undefined): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    return ["https:", "http:"].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined;
  }
}
export function displayTime(
  value: string | null | undefined,
  locale: "pt" | "en",
  full = false,
): string {
  if (!value || !Number.isFinite(Date.parse(value)))
    return locale === "pt" ? "Desconhecido" : "Unknown";
  return new Intl.DateTimeFormat(locale === "pt" ? "pt-PT" : "en-GB", {
    timeZone: "Europe/Lisbon",
    hour: "2-digit",
    minute: "2-digit",
    ...(full ? { day: "2-digit", month: "short", second: "2-digit" } : {}),
  }).format(new Date(value));
}

export function forecastFeatures(
  snapshot: Snapshot,
): GeoJSON.FeatureCollection {
  const features: GeoJSON.Feature[] = [];
  for (const forecast of snapshot.forecasts) {
    const geo = forecast.geometry as GeoJSON.GeoJSON;
    if (geo.type === "FeatureCollection") features.push(...geo.features);
    else if (geo.type === "Feature") features.push(geo);
    else
      features.push({
        type: "Feature",
        properties: { id: forecast.id },
        geometry: geo,
      });
  }
  return { type: "FeatureCollection", features };
}
