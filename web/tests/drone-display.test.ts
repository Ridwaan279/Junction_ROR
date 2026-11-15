import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { deploymentsAt, missionAt } from "../lib/drone-display";
import { isLiveStatus } from "../lib/live-data";
import type { AgentDrone, ReplayFrame, ReplayIndex } from "../lib/replay";

describe("deployment times and mission phases", () => {
  const index = JSON.parse(readFileSync(join(__dirname,"../public/generated/arganil-early/index.json"),"utf8")) as ReplayIndex;
  const loaded = Object.fromEntries(index.frames.map((f) => [f.file, JSON.parse(readFileSync(join(__dirname,"../public/generated/arganil-early",f.file),"utf8")) as ReplayFrame]));
  it("excludes reserves, deduplicates stable IDs, and hides future deployments after seeking", () => {
    const first = Date.parse(index.frames[1].t);
    expect(deploymentsAt(index, loaded, Date.parse(index.frames[0].t))).toHaveLength(0);
    const early = deploymentsAt(index, loaded, first);
    expect(early).toHaveLength(2);
    expect(early.every((d) => d.at === first && d.drone.route !== null)).toBe(true);
    const later = deploymentsAt(index, loaded, Date.parse(index.window.to));
    expect(later.length).toBeGreaterThan(early.length);
    expect(new Set(later.map((d) => d.drone.id)).size).toBe(later.length);
    expect(deploymentsAt(index, loaded, first)).toHaveLength(2);
  });
  it("distinguishes ground transfer, flight, return and completed missions", () => {
    const drone: AgentDrone = {id:"synthetic-test",station_id:"synthetic",role:"perimeter",task_point:[-7.98,40],route:{duration_s:40,detour_reason:null,
      waypoints:[{position:[-8,40,0],t_s:0},{position:[-7.99,40,0],t_s:10},{position:[-7.98,40,120],t_s:20},{position:[-7.99,40,0],t_s:30},{position:[-8,40,0],t_s:40}],
      legs:[{kind:"ground_transfer",from:0,to:1},{kind:"flight",from:1,to:3},{kind:"ground_transfer",from:3,to:4}]}};
    expect(missionAt(drone,0,-1)).toBeNull();
    expect(missionAt(drone,0,5000)?.phase).toBe("ground_transfer");
    expect(missionAt(drone,0,15000)?.phase).toBe("flight");
    expect(missionAt(drone,0,15000)?.position[2]).toBe(60);
    expect(missionAt(drone,0,25000)?.bearing).toBe(-90);
    expect(missionAt(drone,0,35000)?.phase).toBe("ground_transfer");
    expect(missionAt(drone,0,41000)).toMatchObject({phase:"returned",position:[-8,40,0]});
  });
});
describe("live service boundary", () => {
  const fixture=JSON.parse(readFileSync(join(__dirname,"../public/fixtures/v2/live-status.json"),"utf8"));
  it("rejects recorded captures and malformed responses instead of displaying them as live", () => {
    expect(isLiveStatus(fixture)).toBe(false);
    expect(isLiveStatus({...fixture,origin:"service"})).toBe(true);
    expect(isLiveStatus({...fixture,origin:"service",checked_at:"invalid"})).toBe(false);
    expect(isLiveStatus({...fixture,origin:"service",active:[{place:{unsafe:true}}]})).toBe(false);
  });
});
