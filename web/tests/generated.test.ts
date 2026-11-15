import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { frameAt, pinsAt, rescueTargetsAt, type ReplayFrame, type ReplayIndex } from "../lib/replay";

const dir = join(__dirname, "../public/generated/arganil-early");
const index = JSON.parse(readFileSync(join(dir, "index.json"), "utf8")) as ReplayIndex;
const frames = index.frames.map((f) => JSON.parse(readFileSync(join(dir, f.file), "utf8")) as ReplayFrame);

describe("Agent 2 generated replay works with the console", () => {
  it("has the fields the card and map read", () => {
    expect(index.timeline.some((x) => x.status === "Início")).toBe(true);
    expect(index.stations?.length).toBeGreaterThan(0);
    for (const f of frames) {
      expect(f.model.area_ha_p50).toBeTypeOf("number");
      expect(f.predicted).toBeTruthy();
      expect(Array.isArray(f.smoke)).toBe(true);
    }
  });
  it("has fleets whose routes run forward in time and start at a known station", () => {
    const ids = new Set(index.stations!.map((s) => s.id));
    for (const f of frames) for (const d of f.fleet?.drones ?? []) {
      expect(ids.has(d.station_id)).toBe(true);
      const w = d.route?.waypoints ?? [];
      for (let i = 1; i < w.length; i++) expect(w[i].t_s).toBeGreaterThanOrEqual(w[i - 1].t_s);
    }
  });
  it("never selects a frame after the cursor and keeps pins and targets public-only", () => {
    const t = Date.parse("2025-08-13T05:15:00Z");
    expect(Date.parse(index.frames[frameAt(index, t)].t)).toBeLessThanOrEqual(t);
    for (const p of [...pinsAt(index, t), ...rescueTargetsAt(index, t)]) for (const i of p.items) expect(Date.parse(i.known_at)).toBeLessThanOrEqual(t);
  });
});
