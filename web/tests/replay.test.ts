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
