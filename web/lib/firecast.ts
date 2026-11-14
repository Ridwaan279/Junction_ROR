// Helpers for physics forecasts ("firecast") carried in Forecast.geometry.
import type { Forecast, Snapshot } from "./contracts";

export interface FirecastHorizon {
  horizon_h: number;
  area_ha_p10: number;
  area_ha_p50: number;
  area_ha_p90: number;
  max_run_km_p50: number;
  run_bearing_deg: number | null;
}
export interface FirecastSmoke {
  hour_offset: number;
  valid_at: string;
  stability: string;
  solar_elevation_deg: number;
  mixing_height_m: number;
  transport_from_deg: number;
  transport_toward_deg: number;
  transport_mps: number;
  transport_basis: string;
  burned_area_ha: number;
  area_growth_ha_h: number;
  heat_release_mw: number;
  emission_pm25_kg_h: number;
  plume_rise_m: number;
  effective_height_m: number;
  trapped_in_mixing_layer: boolean;
  reach: { threshold_ug_m3: number; reach_km: number }[];
}
export interface FirecastInput {
  kind: string;
  source: string;
  url: string | null;
  via: string | null;
  status: string;
  detail: string;
  captured_at: string;
}
export interface FirecastReport {
  url: string;
  title: string;
  source: string | null;
  published_at: string | null;
  conditions: Record<string, unknown> | null;
}
export interface Sourced<T> {
  value: T;
  evidence: string;
  url: string;
}
export interface FirecastSummary {
  model: string;
  basis: "live" | "historical" | "what_if" | "synthetic";
  ignition_estimate: string;
  now: string;
  elapsed_min: number;
  members: number;
  seed: number;
  cell_m: number;
  domain_km: number;
  runtime_ms: number;
  edge_reached: boolean;
  horizons: FirecastHorizon[];
  head: {
    ros_m_min: number;
    bearing_deg: number;
    intensity_kw_m: number;
    flame_length_m: number;
    length_to_breadth: number;
    effective_wind_kmh: number;
    wind_limited: boolean;
    suppression: string;
    crown_fraction?: number;
    fire_type?: string;
  } | null;
  ignition: {
    position: [number, number];
    uncertainty_m: number;
    elevation_m: number;
    slope_deg: number;
    upslope_deg: number;
    fuel: string;
    fuel_model: string | null;
  };
  weather: {
    valid_at: string;
    temp_c: number;
    rh_pct: number;
    wind_mps: number;
    wind_from_deg: number;
    gust_mps: number | null;
    wind_120m_mps: number | null;
    wind_120m_from_deg: number | null;
    cloud_pct: number | null;
    shortwave_wm2: number | null;
    vpd_kpa: number | null;
    solar_elevation_deg: number;
    hourly: { t: string; wind_mps: number; from_deg: number; gust_mps: number | null; temp_c: number; rh_pct: number }[];
  };
  moisture: {
    dead_1h_pct: number;
    dead_10h_pct: number;
    dead_100h_pct: number;
    live_pct: number;
    basis: string[];
  };
  terrain: { relief_m: number };
  fuel: { fractions: Record<string, number>; mapped_fraction: number; ignition_model: string | null };
  smoke: FirecastSmoke[];
  intel: {
    reports: FirecastReport[];
    regional: {
      dominant_vegetation: string[];
      drought_status: string | null;
      local_wind_regime: string | null;
      active_weather_warnings: string[];
      recent_burn_scars: string[];
      notes: string | null;
      sources: string[];
    } | null;
    credits_used: number;
    calls: string[];
    errors: string[];
    adjustments: {
      regional_cover: Sourced<string> | null;
      drought_live_reduction: Sourced<number> | null;
      reported_spread_bearing: Sourced<number> | null;
      reported_wind_from_deg: Sourced<number> | null;
      threatened_places: Sourced<string>[];
      barriers: Sourced<string>[];
      evacuations: Sourced<string>[];
    };
    agreement: {
      reported_bearing_deg: number;
      model_run_bearing_deg: number;
      delta_deg: number;
      verdict: string;
      evidence: string;
      url: string;
    } | null;
  };
  warnings: { area: string; kind: string; level: string; text: string }[];
  inputs: FirecastInput[];
  references: string[];
}

type Feature = GeoJSON.Feature<GeoJSON.Geometry, Record<string, unknown>>;

export function isFirecast(forecast: Forecast): boolean {
  return forecast.model_version.startsWith("firecast");
}
export function summaryOf(forecast: Forecast | undefined): FirecastSummary | null {
  if (!forecast || !isFirecast(forecast)) return null;
  const geo = forecast.geometry as unknown as { firecast?: FirecastSummary };
  return geo.firecast ?? null;
}
export function firecastFor(snapshot: Snapshot, incidentId: string | null | undefined): Forecast | undefined {
  const all = snapshot.forecasts.filter(isFirecast);
  return all.find((f) => f.incident_id === incidentId) ?? all[0];
}
export function featuresOf(forecast: Forecast | undefined, layer: string): Feature[] {
  if (!forecast) return [];
  const geo = forecast.geometry as unknown as GeoJSON.FeatureCollection;
  if (geo.type !== "FeatureCollection") return [];
  return (geo.features as Feature[]).filter((f) => f.properties?.layer === layer);
}
export function horizonsOf(forecast: Forecast | undefined): number[] {
  const set = new Set(featuresOf(forecast, "spread").map((f) => Number(f.properties.horizon_h)));
  return [...set].sort((a, b) => a - b);
}

const COMPASS_PT = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSO", "SO", "OSO", "O", "ONO", "NO", "NNO"];
const COMPASS_EN = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
export function compass(deg: number | null | undefined, locale: "pt" | "en"): string {
  if (deg === null || deg === undefined || !Number.isFinite(deg)) return "—";
  const i = Math.round((((deg % 360) + 360) % 360) / 22.5) % 16;
  return (locale === "pt" ? COMPASS_PT : COMPASS_EN)[i];
}

/** Arrow (shaft + two head strokes) for a LineString in lon/lat. */
export function arrowPaths(line: [number, number][], headFraction = 0.32): [number, number][][] {
  if (line.length < 2) return [];
  const [a, b] = [line[0], line[line.length - 1]];
  const k = Math.cos((a[1] * Math.PI) / 180);
  const dx = (b[0] - a[0]) * k,
    dy = b[1] - a[1];
  const len = Math.hypot(dx, dy);
  if (len === 0) return [];
  const angle = Math.atan2(dy, dx);
  const head = len * headFraction;
  const side = (rot: number): [number, number] => [
    b[0] + (head * Math.cos(angle + Math.PI + rot)) / k,
    b[1] + head * Math.sin(angle + Math.PI + rot),
  ];
  return [
    [a, b],
    [side(0.45), b, side(-0.45)],
  ];
}

/** Progressive colours: hot near-term, cooler further out. */
export function horizonColor(h: number): [number, number, number] {
  if (h <= 0) return [140, 18, 18];
  if (h <= 1) return [222, 45, 25];
  if (h <= 3) return [246, 128, 32];
  return [252, 205, 64];
}
export function smokeAlpha(threshold: number): number {
  if (threshold >= 500) return 150;
  if (threshold >= 150) return 100;
  return 55;
}
