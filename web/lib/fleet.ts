/**
 * Deterministic drone fleet sizing for the replay (simulation; demonstration constants).
 * Inputs are only data already on screen at time t: modelled active front, ERA5 wind,
 * and at-risk places / nearby events named in verified reports.
 */
export const FLEET = {
  cruise_kmh: 54,          // 15 m/s ground speed
  revisit_min: 10,         // each point of the front seen every 10 min
  head_wind_step_kmh: 20,  // +1 head scout per 20 km/h of wind (spotting distance grows with wind)
  sites_per_drone: 3,      // one overwatch drone per three nearby places
  site_radius_km: 15,
  endurance_min: 60,
  recharge_min: 20,
  staging_km: 5,           // staging point upwind of the fire
  reserve: 0.2,
  gust_limit_kmh: 50,      // above this, small drones should not fly
} as const;

export type FleetPlan = {
  segment_km: number; front_km: number; wind_kmh: number; gust_kmh: number | null; sites: number;
  perimeter: number; head: number; overwatch: number; airborne: number;
  transit_min: number; duty: number; total: number; grounded: boolean;
};

export function planFleet(front_km: number, wind_kmh: number, gust_kmh: number | null, sites: number): FleetPlan {
  const segment_km = (FLEET.cruise_kmh * (FLEET.revisit_min / 60)) / 2;
  const perimeter = Math.max(1, Math.ceil(front_km / segment_km));
  const head = 1 + Math.floor(Math.max(0, wind_kmh) / FLEET.head_wind_step_kmh);
  const overwatch = Math.ceil(Math.max(0, sites) / FLEET.sites_per_drone);
  const airborne = perimeter + head + overwatch;
  const transit_min = (FLEET.staging_km / FLEET.cruise_kmh) * 60;
  const duty = (FLEET.endurance_min - 2 * transit_min) / (FLEET.endurance_min + FLEET.recharge_min);
  const total = Math.ceil((airborne / duty) * (1 + FLEET.reserve));
  return {
    segment_km, front_km, wind_kmh, gust_kmh, sites, perimeter, head, overwatch, airborne,
    transit_min, duty, total, grounded: gust_kmh != null && gust_kmh > FLEET.gust_limit_kmh,
  };
}

type LonLat = [number, number];
const M_PER_DEG_LAT = 111_320;
const toXY = (p: LonLat, o: LonLat): [number, number] => [(p[0] - o[0]) * M_PER_DEG_LAT * Math.cos((o[1] * Math.PI) / 180), (p[1] - o[1]) * M_PER_DEG_LAT];
const toLL = (x: number, y: number, o: LonLat): LonLat => [o[0] + x / (M_PER_DEG_LAT * Math.cos((o[1] * Math.PI) / 180)), o[1] + y / M_PER_DEG_LAT];
export const distanceKm = (a: LonLat, b: LonLat) => Math.hypot(...toXY(a, b)) / 1000;

/** Staging point FLEET.staging_km upwind of the ignition, in the clean air the wind blows from. */
export function stagingPoint(ignition: LonLat, wind_from_deg: number): LonLat {
  const r = (wind_from_deg * Math.PI) / 180, d = FLEET.staging_km * 1000;
  return toLL(d * Math.sin(r), d * Math.cos(r), ignition);
}

export type DroneTask = { role: "perimeter" | "head" | "overwatch"; target: LonLat };

/** Where each airborne drone works: evenly along the modelled front, at the far end of the 6 h forecast, over groups of places. */
export function droneTasks(plan: FleetPlan, ignition: LonLat, front: LonLat[] | null, forecast6h: LonLat[] | null, places: LonLat[]): DroneTask[] {
  const tasks: DroneTask[] = [];
  const ring = front && front.length > 2 ? front : null;
  for (let i = 0; i < plan.perimeter; i++) {
    tasks.push({ role: "perimeter", target: ring ? alongRing(ring, (i + 0.5) / plan.perimeter) : ignition });
  }
  const head = forecast6h && forecast6h.length ? forecast6h.reduce((a, b) => (distanceKm(b, ignition) > distanceKm(a, ignition) ? b : a)) : ignition;
  for (let i = 0; i < plan.head; i++) {
    const [x, y] = toXY(head, ignition), len = Math.hypot(x, y) || 1, side = (i - (plan.head - 1) / 2) * 600;
    tasks.push({ role: "head", target: toLL(x + (-y / len) * side, y + (x / len) * side, ignition) });
  }
  const near = [...places].sort((a, b) => distanceKm(a, ignition) - distanceKm(b, ignition));
  for (let i = 0; i < plan.overwatch; i++) {
    const group = near.slice(i * FLEET.sites_per_drone, (i + 1) * FLEET.sites_per_drone);
    if (!group.length) break;
    tasks.push({ role: "overwatch", target: [group.reduce((s, p) => s + p[0], 0) / group.length, group.reduce((s, p) => s + p[1], 0) / group.length] });
  }
  return tasks;
}

function alongRing(ring: LonLat[], f: number): LonLat {
  const seg = ring.slice(1).map((p, i) => distanceKm(ring[i], p));
  let target = seg.reduce((a, b) => a + b, 0) * f;
  for (let i = 0; i < seg.length; i++) {
    if (target <= seg[i]) { const k = seg[i] ? target / seg[i] : 0; return [ring[i][0] + (ring[i + 1][0] - ring[i][0]) * k, ring[i][1] + (ring[i + 1][1] - ring[i][1]) * k]; }
    target -= seg[i];
  }
  return ring[ring.length - 1];
}

/** Largest outer ring of a (Multi)Polygon, or null. */
export function outerRing(geom: GeoJSON.Polygon | GeoJSON.MultiPolygon | null): LonLat[] | null {
  if (!geom) return null;
  const polys = geom.type === "Polygon" ? [geom.coordinates] : geom.coordinates;
  let best: LonLat[] | null = null;
  for (const p of polys) if (!best || p[0].length > best.length) best = p[0] as LonLat[];
  return best;
}
