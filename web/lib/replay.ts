import type { Map as LibreMap, GeoJSONSource } from "maplibre-gl";
import { droneImage, missionAt } from "./drone-display";

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
  automation?: AutomationStep[];
  stations?: Station[];
};
export type Station = { id: string; name: string; position: [number, number]; simulated: boolean; drones_total: number; drones_available: number; coordinate_source: string };
export type Waypoint = { position: [number, number, number]; t_s: number };
export type AgentDrone = { id: string; role: string; station_id: string; task_point: [number, number];
  route: { waypoints: Waypoint[]; legs: { kind: "flight" | "ground_transfer"; from: number; to: number }[]; detour_reason: string | null; duration_s?: number } | null };
export type AgentFleet = { at: string; count: number; previous_count: number | null; change_reason_pt: string | null; change_reason_en: string | null;
  breakdown: { term: string; value: number; unit: string; basis: string }[]; drones: AgentDrone[]; held: unknown[] };
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
  fleet?: AgentFleet | null;
  drones_legacy: { count: number; bases: [number, number][]; routes: { base: number; eta_min: number; path: [number, number][]; role: string }[] };
};
export type AutomationStep = { step: string; at: string; basis: "observed" | "simulated"; detail: string; url: string | null };

/** Agent 2's generated replay (fleet, stations, routes); the hand-built fixture is the fallback. */
export const REPLAY_BASES = ["/generated/arganil-early/", "/fixtures/v2/arganil-early/"] as const;
export const REPLAY_BASE = REPLAY_BASES[1];

const empty: GeoJSON.FeatureCollection = { type: "FeatureCollection", features: [] };
const feature = (geometry: Geometry | null, properties: Record<string, unknown> = {}): GeoJSON.Feature[] =>
  geometry ? [{ type: "Feature", geometry, properties }] : [];

export const FIRE = { p10: "#ffb547", p50: "#ff6a2b", p90: "#e3261f", envelope: "#ffd166", smoke: "#c7b9a8", drone: "#64d2ff" };

/** Native MapLibre layers drape onto the 3D terrain (deck.gl overlays would float). */
export function addReplayLayers(map: LibreMap) {
  if (map.getSource("rp-fire")) return;
  if (!map.hasImage("rp-quadcopter")) map.addImage("rp-quadcopter", droneImage(), { pixelRatio: 2 });
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
    "line-color": "#ffd0a1", "line-width": 1 } });
  map.addLayer({ id: "rp-routes-ground", type: "line", source: "rp-routes", filter: ["==", ["get", "kind"], "ground_transfer"], paint: {
    "line-color": FIRE.drone, "line-width": 1, "line-opacity": 0.45, "line-dasharray": [2, 2] } });
  map.addLayer({ id: "rp-routes", type: "line", source: "rp-routes", filter: ["!=", ["get", "kind"], "ground_transfer"], layout: { "line-cap": "round" }, paint: {
    "line-color": FIRE.drone, "line-width": 1.4, "line-opacity": 0.75 } });
  map.addLayer({ id: "rp-bases", type: "circle", source: "rp-bases", paint: {
    "circle-radius": 3, "circle-color": "rgba(0,0,0,0.5)", "circle-stroke-color": FIRE.drone, "circle-stroke-width": 1.2 } });
  map.addLayer({ id: "rp-drones", type: "symbol", source: "rp-drones", layout: {
    "icon-image": "rp-quadcopter", "icon-size": ["interpolate", ["linear"], ["zoom"], 6, 0.45, 12, 0.7, 18, 1],
    "icon-allow-overlap": true, "icon-ignore-placement": true,
    "icon-rotate": ["coalesce", ["get", "bearing"], 0], "icon-rotation-alignment": "map", "icon-pitch-alignment": "viewport",
  }, paint: { "icon-opacity": ["case", ["==", ["get", "phase"], "ground_transfer"], 0.72, 1] } });
  map.addLayer({ id: "rp-villages", type: "circle", source: "rp-villages", paint: {
    "circle-radius": ["interpolate", ["linear"], ["zoom"], 8, 3.5, 13, 7],
    "circle-color": ["case", ["get", "evacuated"], "#ff453a", "#ffd60a"],
    "circle-stroke-color": "rgba(0,0,0,0.6)", "circle-stroke-width": 1, "circle-opacity": 0.95 } });
  map.addLayer({ id: "rp-village-labels", type: "symbol", source: "rp-villages", minzoom: 10.2, layout: {
    "text-field": ["get", "name"], "text-font": ["Noto Sans Regular"], "text-size": 11.5,
    "text-offset": [0, 1.1], "text-anchor": "top", "text-optional": true }, paint: {
    "text-color": "#ffffff", "text-halo-color": "rgba(5,8,12,0.9)", "text-halo-width": 1.4 } });
  map.addLayer({ id: "rp-ignition", type: "circle", source: "rp-ignition", paint: {
    "circle-radius": 4, "circle-color": "#ffffff", "circle-stroke-color": "rgba(0,0,0,0.6)", "circle-stroke-width": 1 } });
}

export const PIN_COLORS = { event: "#bf5af2", shelter: "#30d158", road: "#ff9f0a", people: "#0a84ff" } as const;
export type PinKind = keyof typeof PIN_COLORS;

/** Pin element for a MapLibre Marker (DOM markers stay visible when deep valleys hide map symbols in 3D). */
export function pinElement(kind: PinKind, label: string) {
  const el = document.createElement("button");
  el.type = "button";
  el.setAttribute("aria-label", label);
  el.style.cssText = "width:18px;height:24px;padding:0;border:0;background:none;cursor:pointer;filter:drop-shadow(0 1px 2px rgba(0,0,0,.6))";
  el.innerHTML = `<svg viewBox="0 0 18 24" width="18" height="24" aria-hidden="true"><path d="M9 23.5C9 23.5 1 14.6 1 9a8 8 0 1 1 16 0c0 5.6-8 14.5-8 14.5Z" fill="${PIN_COLORS[kind]}" stroke="rgba(0,0,0,.55)" stroke-width="1"/><circle cx="9" cy="9" r="3.2" fill="#fff"/></svg>`;
  return el;
}

export type Pin = { id: string; kind: PinKind; place: string; position: [number, number];
  items: { quote: string; statement_en: string; url: string; publisher: string; known_at: string }[] };
const SHELTER_WORDS = /abrig|acolhiment|casa do povo|centro social|encaminhad|\blares?\b|igreja/i;
const EVENT_WORDS = /\b(festa|festas|feira|romaria|festival|arraial|procissão|concerto|evento)\b/i;
const PIN_OFFSET: Record<PinKind, number> = { event: 0, shelter: 0.0045, road: -0.0045, people: 0.0025 };

/** Events, shelters and road closures named in verified reports already public at t. */
export function pinsAt(index: ReplayIndex, t: number): Pin[] {
  const out = new Map<string, Pin>();
  for (const e of index.evidence) {
    if (e.quality !== "verified" || Date.parse(e.known_at) > t) continue;
    const kind: PinKind | null = e.category === "event_gathering" && EVENT_WORDS.test(e.quote) ? "event"
      : e.category === "shelter" && SHELTER_WORDS.test(e.quote) ? "shelter"
      : e.category === "road_closure" ? "road"
      : e.category === "people_present" ? "people" : null;
    if (!kind) continue;
    for (const p of e.places) {
      if (!p.geonames || !p.position || MUNICIPALITIES.has(p.geonames)) continue;
      const id = kind + ":" + p.geonames;
      const pin = out.get(id) ?? { id, kind, place: p.geonames, position: [p.position[0] + PIN_OFFSET[kind], p.position[1]], items: [] };
      if (!pin.items.some((x) => x.url === e.url && x.quote === e.quote))
        pin.items.push({ quote: e.quote, statement_en: e.statement_en, url: e.url, publisher: e.publisher, known_at: e.known_at });
      out.set(id, pin);
    }
  }
  return [...out.values()];
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
  for (const id of ["rp-smoke", "rp-forecast", "rp-fire-fill", "rp-fire-edge", "rp-routes-ground", "rp-routes", "rp-bases", "rp-drones", "rp-villages", "rp-village-labels", "rp-ignition"])
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

/** Simulated fleet: staging point, a line to each drone's task, and the drone's current position. */
export function showFleet(map: LibreMap, base: [number, number] | null, drones: { position: [number, number]; target: [number, number]; role: string }[]) {
  const set = (id: string, features: GeoJSON.Feature[]) =>
    (map.getSource(id) as GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features });
  set("rp-bases", base && drones.length ? [{ type: "Feature", geometry: { type: "Point", coordinates: base }, properties: {} }] : []);
  set("rp-routes", base ? drones.map((d) => ({ type: "Feature", geometry: { type: "LineString", coordinates: [base, d.target] }, properties: { role: d.role } })) : []);
  set("rp-drones", drones.map((d,i) => ({ type: "Feature", geometry: { type: "Point", coordinates: d.position }, properties: {
    id: `demo-${i+1}`, role: d.role, phase: "flight", bearing: Math.atan2(d.target[0]-d.position[0],d.target[1]-d.position[1])*180/Math.PI,
  } })));
}

/** Model-versus-observed comparison per GWIS perimeter (multi-day replay). */
export type ValidationRow = {
  cycle: number; from: string; to: string; hours: number; started_from: string;
  observed_area_ha: number; predicted_area_ha_p10: number; predicted_area_ha_p50: number; predicted_area_ha_p90: number;
  dice_total: number; dice_growth: number; observed_growth_bearing_deg: number; predicted_growth_bearing_deg: number;
};

/** Search-and-rescue targets: places where verified reports put people (events, shelters, people still there). */
export type RescueTarget = Pin & { latest: string; people: number | null; distance_km: number };
export function rescueTargetsAt(index: ReplayIndex, t: number): RescueTarget[] {
  const ig = index.ignition.position, k = Math.cos((ig[1] * Math.PI) / 180);
  const people = new Map<string, number>();
  for (const e of index.evidence) {
    if (e.quality !== "verified" || e.category !== "people_present" || Date.parse(e.known_at) > t || e.number == null) continue;
    const named = e.places.filter((p) => p.geonames && !MUNICIPALITIES.has(p.geonames));
    if (named.length === 1 && /pessoa|habitante|morador|residente/i.test(e.unit ?? "")) people.set(named[0].geonames!, e.number);
  }
  const words: Record<string, RegExp> = { event: EVENT_WORDS, shelter: SHELTER_WORDS, people: /pessoa|habitante|morador|idos|residente|popula/i };
  const at = (q: string, re: RegExp) => { const m = re.exec(q); return m ? m.index : 9999; };
  return pinsAt(index, t)
    .filter((p) => p.kind !== "road")
    .map((p) => ({
      ...p,
      // Show the most on-topic report first: the one where the target's words appear earliest.
      items: [...p.items].sort((a, b) => at(a.quote, words[p.kind]) - at(b.quote, words[p.kind])),
      latest: p.items.reduce((a, b) => (b.known_at > a ? b.known_at : a), ""),
      people: p.kind === "people" ? people.get(p.place) ?? null : null,
      distance_km: Math.hypot((p.position[0] - ig[0]) * k, p.position[1] - ig[1]) * 111.32,
    }))
    .sort((a, b) => a.distance_km - b.distance_km);
}

/** Agent 2 fleet: stations in use, the outbound path of each drone (ground leg dashed, flight solid) and its position at t. */
export function showAgentFleet(map: LibreMap, stations: Station[], drones: AgentDrone[], launches: Map<string, number>, t: number) {
  const set = (id: string, features: GeoJSON.Feature[]) =>
    (map.getSource(id) as GeoJSONSource | undefined)?.setData({ type: "FeatureCollection", features });
  const routes: GeoJSON.Feature[] = [], points: GeoJSON.Feature[] = [], used = new Set<string>();
  for (const d of drones) {
    const launch = launches.get(d.id);
    if (launch == null || launch > t) continue;
    const sample = missionAt(d, launch, t), w = d.route?.waypoints ?? [];
    if (!sample || sample.phase === "returned") continue;
    used.add(d.station_id);
    for (const leg of d.route!.legs) {
      if (leg.from >= leg.to) continue;
      routes.push({ type: "Feature", geometry: { type: "LineString", coordinates: w.slice(leg.from, leg.to + 1).map((p) => p.position) }, properties: { kind: leg.kind } });
    }
    points.push({ type: "Feature", geometry: { type: "Point", coordinates: sample.position }, properties: {
      id: d.id, role: d.role, phase: sample.phase, bearing: sample.bearing,
      altitude_m: sample.position[2], deployed_at: new Date(launch).toISOString(),
    } });
  }
  set("rp-routes", routes); set("rp-drones", points);
  set("rp-bases", stations.filter((st) => used.has(st.id)).map((st) => ({ type: "Feature", geometry: { type: "Point", coordinates: st.position }, properties: { name: st.name } })));
}
