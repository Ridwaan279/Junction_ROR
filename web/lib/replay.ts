import type { Map as LibreMap, GeoJSONSource } from "maplibre-gl";

/** Shapes of web/public/fixtures/v2/arganil-early (docs/DEMO_DATA.md). */
export type Geometry = GeoJSON.MultiPolygon | GeoJSON.Polygon;
export type ReplayEvidence = {
  id: string; page: string; category: string; statement_en: string; quote: string;
  places: { name: string; geonames: string | null; position: [number, number] | null }[];
  number: number | null; unit: string | null; quality: "verified" | "unverified";
  known_at: string; url: string; publisher: string; lineage: string | null;
};
export type ReplayPage = { id: string; url: string; publisher: string; headline: string | null; status: string; known_at: string | null; error_code: string | null };
export type Monitor = { id: string; category: string; provider: string; target: string; status: string; items: number; note: string };
export type ReplayIndex = {
  dataset: string; mode: "historical"; title: string; window: { from: string; to: string };
  ignition: { position: [number, number]; url: string; start_local: string; nature: string; place: string };
  timeline: { at: string; status: string; url: string }[];
  frames: { t: string; file: string; evidence: number; exposures: number; area_ha_p50: number }[];
  pages: ReplayPage[]; evidence: ReplayEvidence[]; monitors: Monitor[]; limitations: string[];
  automation: AutomationStep[];
};
export type Exposure = {
  id: string; name: string; kind: string[]; position: [number, number]; people: number | null;
  distance_from_ignition_km: number; events: { name: string | null; scheduled_start_date: string | null; status: string | null }[];
  evidence: string[]; in_modelled_burn_p50: boolean; in_forecast_6h: boolean; in_forecast_12h: boolean;
};
export type ReplayFrame = {
  t: string;
  weather: { temp_c: number; rh_pct: number; wind_mps: number; wind_from_deg: number; gust_mps: number | null } | null;
  model: { area_ha_p10: number; area_ha_p50: number; area_ha_p90: number; front_km: number; growth_ha_h: number; head_ros_m_min: number; head_intensity_kw_m: number; fire_type: string };
  predicted: Record<"p10" | "p50" | "p90", Geometry | null>;
  forecast: Record<"h6" | "h12", Geometry | null>;
  smoke: { threshold_ug_m3: number; geometry: Geometry }[];
  fogos_status: { at: string; status: string } | null;
  evidence_ids: string[];
  exposures: Exposure[];
  automation: AutomationStep[];
  drones_legacy: { count: number; bases: [number, number][]; routes: { base: number; eta_min: number; path: [number, number][]; role: string }[] };
};
export type AutomationStep = { step: string; at: string; basis: "observed" | "simulated"; detail: string; url: string | null };

export const REPLAY_BASE = "/fixtures/v2/arganil-early/";

const empty: GeoJSON.FeatureCollection = { type: "FeatureCollection", features: [] };
const feature = (geometry: Geometry | null, properties: Record<string, unknown> = {}): GeoJSON.Feature[] =>
  geometry ? [{ type: "Feature", geometry, properties }] : [];

export const FIRE = { p10: "#ffb547", p50: "#ff6a2b", p90: "#e3261f", envelope: "#ffd166", smoke: "#c7b9a8", drone: "#5ee0ff" };

/** Native MapLibre layers drape onto the 3D terrain (deck.gl overlays would float). */
export function addReplayLayers(map: LibreMap) {
  if (map.getSource("rp-fire")) return;
  for (const id of ["rp-smoke", "rp-forecast", "rp-fire", "rp-villages", "rp-ignition", "rp-routes", "rp-bases", "rp-drones"]) map.addSource(id, { type: "geojson", data: empty });
  map.addLayer({ id: "rp-smoke", type: "fill", source: "rp-smoke", paint: {
    "fill-color": FIRE.smoke,
    "fill-opacity": ["interpolate", ["linear"], ["get", "threshold"], 35, 0.16, 150, 0.24, 500, 0.32] } });
  map.addLayer({ id: "rp-forecast", type: "line", source: "rp-forecast", paint: {
    "line-color": FIRE.envelope, "line-width": ["match", ["get", "h"], 6, 2, 1.4],
    "line-opacity": ["match", ["get", "h"], 6, 0.95, 0.6], "line-dasharray": [2, 1.6] } });
  map.addLayer({ id: "rp-fire-fill", type: "fill", source: "rp-fire", paint: {
    "fill-color": ["match", ["get", "band"], "p90", FIRE.p90, "p50", FIRE.p50, FIRE.p10],
    "fill-opacity": ["match", ["get", "band"], "p90", 0.62, "p50", 0.42, 0.24] } });
  map.addLayer({ id: "rp-fire-edge", type: "line", source: "rp-fire", filter: ["==", ["get", "band"], "p50"], paint: {
    "line-color": "#ffd0a1", "line-width": 1.6, "line-blur": 0.6 } });
  map.addLayer({ id: "rp-routes", type: "line", source: "rp-routes", layout: { "line-cap": "round" }, paint: {
    "line-color": FIRE.drone, "line-width": 1.6, "line-opacity": 0.75, "line-dasharray": [1, 1.8] } });
  map.addLayer({ id: "rp-bases", type: "circle", source: "rp-bases", paint: {
    "circle-radius": 4, "circle-color": "#0b1218", "circle-stroke-color": FIRE.drone, "circle-stroke-width": 1.6 } });
  map.addLayer({ id: "rp-drones-glow", type: "circle", source: "rp-drones", paint: {
    "circle-radius": 9, "circle-color": "rgba(94, 224, 255, 0.22)", "circle-blur": 0.6 } });
  map.addLayer({ id: "rp-drones", type: "circle", source: "rp-drones", paint: {
    "circle-radius": 3.6, "circle-color": "#ffffff", "circle-stroke-color": FIRE.drone, "circle-stroke-width": 1.6 } });
  map.addLayer({ id: "rp-villages", type: "circle", source: "rp-villages", paint: {
    "circle-radius": ["interpolate", ["linear"], ["zoom"], 8, 3.5, 13, 7],
    "circle-color": ["case", ["get", "evacuated"], "#ff4d4f", "#ffc53d"],
    "circle-stroke-color": "#0b0f14", "circle-stroke-width": 1.5, "circle-opacity": 0.95 } });
  map.addLayer({ id: "rp-village-labels", type: "symbol", source: "rp-villages", minzoom: 10.2, layout: {
    "text-field": ["get", "name"], "text-font": ["Noto Sans Regular"], "text-size": 11.5,
    "text-offset": [0, 1.1], "text-anchor": "top", "text-optional": true }, paint: {
    "text-color": "#ffffff", "text-halo-color": "rgba(5,8,12,0.9)", "text-halo-width": 1.4 } });
  map.addLayer({ id: "rp-ignition-halo", type: "circle", source: "rp-ignition", paint: {
    "circle-radius": 14, "circle-color": "rgba(255, 106, 43, 0.18)", "circle-stroke-color": "rgba(255, 106, 43, 0.7)", "circle-stroke-width": 1 } });
  map.addLayer({ id: "rp-ignition", type: "circle", source: "rp-ignition", paint: {
    "circle-radius": 4.5, "circle-color": "#ffffff", "circle-stroke-color": "#ff6a2b", "circle-stroke-width": 2.5 } });
}

export type Village = { name: string; position: [number, number]; evacuated: boolean; evidence: string[] };
const MUNICIPALITIES = new Set(["Arganil", "Seia", "Oliveira do Hospital", "Pampilhosa da Serra"]);
const IMPACT = new Set(["evacuation", "threatened_place", "shelter", "people_present", "road_closure", "event_gathering"]);

/** Villages named in verified facts already public at time t (minute resolution, not frame resolution). */
export function villagesAt(index: ReplayIndex, t: number): Village[] {
  const out = new Map<string, Village>();
  for (const e of index.evidence) {
    if (e.quality !== "verified" || !IMPACT.has(e.category) || Date.parse(e.known_at) > t) continue;
    for (const p of e.places) {
      if (!p.geonames || !p.position || MUNICIPALITIES.has(p.geonames)) continue;
      const v = out.get(p.geonames) ?? { name: p.geonames, position: p.position, evacuated: false, evidence: [] };
      v.evacuated ||= e.category === "evacuation";
      v.evidence.push(e.id);
      out.set(p.geonames, v);
    }
  }
  return [...out.values()];
}

export function showVillages(map: LibreMap, villages: Village[]) {
  (map.getSource("rp-villages") as GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features: villages.map((v) => ({
    type: "Feature", geometry: { type: "Point", coordinates: v.position }, properties: { name: v.name, evacuated: v.evacuated } })) });
}

export function showReplayFrame(map: LibreMap, frame: ReplayFrame, ignition: [number, number]) {
  const set = (id: string, features: GeoJSON.Feature[]) =>
    (map.getSource(id) as GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features });
  // The 35 µg/m³ band reaches the model-domain edge and reads as a box; show denser smoke only.
  set("rp-smoke", frame.smoke.filter((s) => s.threshold_ug_m3 >= 150).flatMap((s) => feature(s.geometry, { threshold: s.threshold_ug_m3 })));
  set("rp-forecast", [...feature(frame.forecast.h12, { h: 12 }), ...feature(frame.forecast.h6, { h: 6 })]);
  set("rp-fire", (["p10", "p50", "p90"] as const).flatMap((band) => feature(frame.predicted[band], { band })));
  set("rp-ignition", [{ type: "Feature", geometry: { type: "Point", coordinates: ignition }, properties: {} }]);
}

export function setReplayVisible(map: LibreMap, visible: boolean) {
  for (const id of ["rp-smoke", "rp-forecast", "rp-fire-fill", "rp-fire-edge", "rp-routes", "rp-bases", "rp-drones-glow", "rp-drones", "rp-villages", "rp-village-labels", "rp-ignition-halo", "rp-ignition"])
    if (map.getLayer(id)) map.setLayoutProperty(id, "visibility", visible ? "visible" : "none");
}

/** The frame on screen at time t: the latest frame not after t (no future data). */
export function frameAt(index: ReplayIndex, t: number): number {
  let k = 0;
  index.frames.forEach((f, i) => { if (Date.parse(f.t) <= t) k = i; });
  return k;
}

export const lisbon = (iso: string, locale: "pt" | "en", withDate = false) =>
  new Intl.DateTimeFormat(locale === "pt" ? "pt-PT" : "en-GB", {
    timeZone: "Europe/Lisbon", hour: "2-digit", minute: "2-digit",
    ...(withDate ? { day: "numeric", month: "short" } : {}),
  }).format(new Date(iso));

export const compassPoint = (deg: number, locale: "pt" | "en") => {
  const en = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"], pt = ["N", "NE", "E", "SE", "S", "SO", "O", "NO"];
  return (locale === "pt" ? pt : en)[Math.round((((deg % 360) + 360) % 360) / 45) % 8];
};

/** Simulated drones: staging points, routes, and each drone's position along its route at time t. */
export function showDrones(map: LibreMap, frame: ReplayFrame, t: number) {
  const d = frame.drones_legacy;
  const set = (id: string, features: GeoJSON.Feature[]) =>
    (map.getSource(id) as GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features });
  const routes = d?.routes ?? [];
  const used = new Set(routes.map((r) => r.base));
  set("rp-bases", (d?.bases ?? []).filter((_, i) => used.has(i)).map((b) => ({ type: "Feature", geometry: { type: "Point", coordinates: b }, properties: {} })));
  set("rp-routes", routes.map((r) => ({ type: "Feature", geometry: { type: "LineString", coordinates: r.path }, properties: { role: r.role } })));
  const elapsed = (t - Date.parse(frame.t)) / 60_000;
  set("rp-drones", routes.map((r, i) => {
    // Staggered launches 1 min apart; after arrival the drone holds at its task point.
    const progress = Math.max(0, Math.min(1, (elapsed - i) / Math.max(1, r.eta_min)));
    return { type: "Feature", geometry: { type: "Point", coordinates: along(r.path, progress) }, properties: { role: r.role } };
  }));
}

function along(path: [number, number][], f: number): [number, number] {
  if (path.length < 2) return path[0];
  const seg = path.slice(1).map((p, i) => Math.hypot(p[0] - path[i][0], (p[1] - path[i][1]) * 1.3));
  let target = seg.reduce((a, b) => a + b, 0) * f;
  for (let i = 0; i < seg.length; i++) {
    if (target <= seg[i]) {
      const k = seg[i] ? target / seg[i] : 0;
      return [path[i][0] + (path[i + 1][0] - path[i][0]) * k, path[i][1] + (path[i + 1][1] - path[i][1]) * k];
    }
    target -= seg[i];
  }
  return path[path.length - 1];
}

/** Model-versus-observed comparison per GWIS perimeter (multi-day replay). */
export type ValidationRow = {
  cycle: number; from: string; to: string; hours: number; started_from: string;
  observed_area_ha: number; predicted_area_ha_p10: number; predicted_area_ha_p50: number; predicted_area_ha_p90: number;
  dice_total: number; dice_growth: number; observed_growth_bearing_deg: number; predicted_growth_bearing_deg: number;
};
