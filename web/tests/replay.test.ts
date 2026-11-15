import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { frameAt, villagesAt, type ReplayIndex } from "../lib/replay";

const index = JSON.parse(readFileSync(join(__dirname, "../public/fixtures/v2/arganil-early/index.json"), "utf8")) as ReplayIndex;

describe("Arganil early-stage replay", () => {
  it("never shows a frame from after the cursor", () => {
    for (const minutes of [0, 7, 59, 61, 179, 181, 600, 2000]) {
      const t = Date.parse(index.window.from) + minutes * 60_000;
      const f = index.frames[frameAt(index, t)];
      expect(Date.parse(f.t)).toBeLessThanOrEqual(t);
    }
  });
  it("reveals villages only from verified facts already public", () => {
    const t = Date.parse("2025-08-13T09:00:00Z");
    const ids = new Set(villagesAt(index, t).flatMap((v) => v.evidence));
    for (const e of index.evidence.filter((x) => ids.has(x.id))) {
      expect(e.quality).toBe("verified");
      expect(Date.parse(e.known_at)).toBeLessThanOrEqual(t);
    }
  });
  it("has no villages before the first report and evacuations by the evening of day one", () => {
    expect(villagesAt(index, Date.parse(index.timeline[0].at) - 60_000)).toHaveLength(0);
    expect(villagesAt(index, Date.parse("2025-08-13T20:00:00Z")).some((v) => v.evacuated)).toBe(true);
  });
  it("keeps municipalities out of the village layer", () => {
    const names = villagesAt(index, Date.parse(index.window.to)).map((v) => v.name);
    expect(names).not.toContain("Arganil");
    expect(names).toContain("Piódão");
  });
});

import { pinsAt } from "../lib/replay";
describe("nearby-event pins", () => {
  it("show the Piódão summer festival only after it was reported (14 Aug 18:03 Lisbon)", () => {
    const before = pinsAt(index, Date.parse("2025-08-14T17:00:00Z"));
    const after = pinsAt(index, Date.parse("2025-08-14T17:10:00Z"));
    expect(before.some((p) => p.kind === "event")).toBe(false);
    const fest = after.find((p) => p.kind === "event");
    expect(fest?.place).toBe("Piódão");
    expect(fest?.items[0].url).toContain("observador.pt");
  });
  it("include shelters and road closures from verified reports", () => {
    const pins = pinsAt(index, Date.parse(index.window.to));
    expect(pins.some((p) => p.kind === "shelter" && p.place === "Vide")).toBe(true);
    expect(pins.some((p) => p.kind === "road")).toBe(true);
  });
});

import { rescueTargetsAt } from "../lib/replay";
describe("search-and-rescue targets", () => {
  it("lists places with people, never road closures, sorted by distance", () => {
    const tg = rescueTargetsAt(index, Date.parse(index.window.to));
    expect(tg.length).toBeGreaterThan(2);
    expect(tg.every((x) => x.kind !== "road")).toBe(true);
    for (let i = 1; i < tg.length; i++) expect(tg[i].distance_km).toBeGreaterThanOrEqual(tg[i - 1].distance_km);
    expect(tg.some((x) => x.kind === "event" && x.place === "Piódão")).toBe(true);
  });
  it("keeps a stated head count and reveals nothing early", () => {
    const at = Date.parse("2025-08-13T22:46:00Z");
    expect(rescueTargetsAt(index, at).find((x) => x.place === "Barroja")?.people).toBe(2);
    expect(rescueTargetsAt(index, Date.parse("2025-08-13T22:44:00Z")).some((x) => x.place === "Barroja")).toBe(false);
    for (const x of rescueTargetsAt(index, at)) for (const item of x.items) expect(Date.parse(item.known_at)).toBeLessThanOrEqual(at);
  });
});
