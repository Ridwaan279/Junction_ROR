// Wire fields mirror crates/domain; browser code consumes JSON only.
export type DataMode = "live" | "historical" | "synthetic";
export type SourceHealth =
  | "healthy"
  | "empty_verified"
  | "partial"
  | "stale"
  | "blocked"
  | "failed"
  | "unconfigured"
  | "needs_validation";
export interface Coverage {
  complete: boolean;
  explicit_empty: boolean;
  scope: string;
  notes: string[];
}
export interface Provenance {
  source_id: string;
  source_url: string;
  origin_group: string;
  external_id: string | null;
  event_at: string | null;
  published_at: string | null;
  captured_at: string;
  available_at: string;
  availability_basis: string;
  evidence_hash: string;
  quality_flags: string[];
}
export interface Evidence {
  id: string;
  run_id: string;
  mode: DataMode;
  provenance: Provenance;
  position: [number, number] | null;
  uncertainty_m: number | null;
  location_name: string | null;
  municipality: string | null;
  kind: string;
  raw_status: string | null;
  title: string;
  body: string | null;
  attributes: Record<string, unknown>;
}
export interface Weather {
  id: string;
  run_id: string;
  provenance: Provenance;
  station_id: string | null;
  position: [number, number] | null;
  valid_at: string;
  forecast: boolean;
  temperature_c: number | null;
  relative_humidity_pct: number | null;
  wind_speed_mps: number | null;
  wind_from_deg: number | null;
  gust_mps: number | null;
  fire_risk: number | null;
}
export interface SourceStatus {
  id: string;
  name: string;
  source_url: string;
  via: string | null;
  origin_group: string;
  enabled: boolean;
  health: SourceHealth;
  coverage: Coverage;
  last_success_at: string | null;
  captured_at: string | null;
  data_at: string | null;
  attribution: string;
  notes: string[];
}
export interface Incident {
  id: string;
  run_id: string;
  mode: DataMode;
  title: string;
  location_name: string | null;
  position: [number, number] | null;
  uncertainty_m: number | null;
  status: string;
  evidence_level: string;
  evidence_ids: string[];
  origin_groups: string[];
  first_event_at: string | null;
  updated_at: string;
}
export interface Forecast {
  id: string;
  run_id: string;
  incident_id: string;
  epoch: number;
  generated_at: string;
  input_evidence_ids: string[];
  input_weather_ids: string[];
  model_version: string;
  seed: number;
  horizon_seconds: number;
  illustrative: boolean;
  geometry: GeoJSON.GeoJsonObject;
  limitations: string[];
}
export interface DronePlan {
  id: string;
  run_id: string;
  incident_id: string;
  epoch: number;
  generated_at: string;
  simulated: boolean;
  route: [number, number][];
  return_route: [number, number][];
  distance_m: number;
  duration_seconds: number;
  reserve_pct: number;
  reasons: string[];
}
export interface Briefing {
  id: string;
  run_id: string;
  incident_id: string;
  epoch: number;
  generated_at: string;
  text_pt: string;
  text_en: string;
  citations: string[];
  audio_url: string | null;
  simulated: boolean;
}
export interface Scenario {
  id: string;
  run_id: string;
  name: string;
  mode: DataMode;
  start_at: string;
  end_at: string;
  current_at: string;
  playing: boolean;
  speed: number;
  seed: number;
  epoch: number;
  description: string;
}
export interface Snapshot {
  scenario: Scenario;
  incidents: Incident[];
  evidence: Evidence[];
  weather: Weather[];
  sources: SourceStatus[];
  forecasts: Forecast[];
  drone_plans: DronePlan[];
  briefings: Briefing[];
}
export interface EventEnvelope {
  id: string;
  type: string;
  run_id: string;
  scenario_id: string | null;
  epoch: number;
  event_time: string;
  emitted_at: string;
  payload: unknown;
}
export const PORTUGAL_BBOX = [-9.6, 36.9, -6.1, 42.2] as const;
