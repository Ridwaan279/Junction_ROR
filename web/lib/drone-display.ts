import type { AgentDrone, ReplayFrame, ReplayIndex, Station } from "./replay";

export type Deployment = { at: number; drone: AgentDrone; station: Station | undefined; phase: "flight" | "ground_transfer" };

/** First actual route assignment, excluding inventory held in reserve and future frames. */
export function deploymentsAt(index: ReplayIndex, loaded: Record<string, ReplayFrame>, cursor: number): Deployment[] {
  const seen = new Set<string>(), out: Deployment[] = [];
  for (const summary of [...index.frames].sort((a, b) => Date.parse(a.t) - Date.parse(b.t))) {
    if (Date.parse(summary.t) > cursor) continue;
    const fleet = loaded[summary.file]?.fleet;
    if (!fleet) continue;
    const at = Date.parse(fleet.at);
    if (!Number.isFinite(at) || at > cursor) continue;
    for (const drone of fleet.drones) {
      if (seen.has(drone.id) || !drone.route || drone.route.waypoints.length < 2) continue;
      seen.add(drone.id);
      out.push({ at, drone, station: index.stations?.find((s) => s.id === drone.station_id),
        phase: drone.route.legs[0]?.kind === "ground_transfer" ? "ground_transfer" : "flight" });
    }
  }
  return out;
}

export function missionAt(drone: AgentDrone, deployed: number, cursor: number) {
  const route = drone.route, elapsed = (cursor - deployed) / 1000;
  if (!route || elapsed < 0 || route.waypoints.length < 2) return null;
  const w = route.waypoints;
  if (w.some((p, i) => !Number.isFinite(p.t_s) || p.position.some((v) => !Number.isFinite(v)) || i > 0 && p.t_s < w[i - 1].t_s)) return null;
  const pair = w.slice(1).findIndex((p) => elapsed <= p.t_s);
  const i = pair < 0 ? w.length - 1 : pair + 1;
  const a = w[i - 1], b = w[i];
  const fraction = Math.max(0, Math.min(1, (elapsed - a.t_s) / Math.max(1e-6, b.t_s - a.t_s)));
  const position = a.position.map((v, j) => v + (b.position[j] - v) * fraction) as [number, number, number];
  const phase = elapsed >= (route.duration_s ?? w[w.length - 1].t_s) ? "returned"
    : [...route.legs].reverse().find((l) => elapsed >= w[l.from].t_s && elapsed <= w[l.to].t_s)?.kind ?? route.legs[0]?.kind ?? "flight";
  const bearing = Math.atan2((b.position[0] - a.position[0]) * Math.cos(position[1] * Math.PI / 180), b.position[1] - a.position[1]) * 180 / Math.PI;
  return { position, bearing, phase };
}

/** A small quadcopter sprite, drawn as code so it stays sharp at screen pixel ratio. */
export function droneImage(): ImageData {
  const canvas = document.createElement("canvas"); canvas.width = canvas.height = 96;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("Drone marker unavailable");
  ctx.lineCap = "round";
  ctx.shadowColor = "rgba(0,0,0,.55)"; ctx.shadowBlur = 5; ctx.shadowOffsetY = 3;
  ctx.strokeStyle = "#142b36"; ctx.lineWidth = 10;
  for (const [x,y] of [[23,23],[73,23],[23,73],[73,73]]) {
    ctx.beginPath(); ctx.moveTo(48,48); ctx.lineTo(x,y); ctx.stroke();
  }
  ctx.shadowBlur = 0; ctx.shadowOffsetY = 0;
  for (const [x,y] of [[23,23],[73,23],[23,73],[73,73]]) {
    ctx.fillStyle = "rgba(13,32,45,.88)";ctx.beginPath();ctx.arc(x,y,17,0,Math.PI*2);ctx.fill();
    ctx.strokeStyle = "#85e0f6"; ctx.lineWidth = 2; ctx.stroke();
    ctx.strokeStyle = "#bed7df"; ctx.lineWidth = 3;
    ctx.beginPath();ctx.moveTo(x-11,y-5);ctx.lineTo(x+11,y+5);ctx.moveTo(x-5,y+11);ctx.lineTo(x+5,y-11);ctx.stroke();
    ctx.fillStyle="#edf7f9";ctx.beginPath();ctx.arc(x,y,3,0,Math.PI*2);ctx.fill();
  }
  const body=ctx.createLinearGradient(36,32,59,63);body.addColorStop(0,"#f1f7f8");body.addColorStop(1,"#839da7");
  ctx.fillStyle=body;ctx.strokeStyle="#102a36";ctx.lineWidth=2;
  ctx.beginPath();ctx.roundRect(37,29,22,37,8);ctx.fill();ctx.stroke();
  ctx.fillStyle="#36c9ee";ctx.fillRect(42,39,12,9);
  ctx.fillStyle="#ff9b53";ctx.beginPath();ctx.moveTo(48,24);ctx.lineTo(43,31);ctx.lineTo(53,31);ctx.closePath();ctx.fill();
  ctx.fillStyle="#102a36";ctx.beginPath();ctx.arc(48,62,4,0,Math.PI*2);ctx.fill();
  return ctx.getImageData(0,0,96,96);
}
