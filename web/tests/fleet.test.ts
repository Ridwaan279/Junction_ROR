import { describe, expect, it } from "vitest";
import { droneTasks, FLEET, planFleet, stagingPoint, distanceKm } from "../lib/fleet";

describe("drone fleet sizing", () => {
  it("matches a hand calculation", () => {
    // 6.2 km front / 4.5 km = 2; 12 km/h wind -> 1 head scout; 15 places / 3 = 5; 8 airborne.
    const p = planFleet(6.2, 12, 32, 15);
    expect(p.segment_km).toBeCloseTo(4.5);
    expect([p.perimeter, p.head, p.overwatch, p.airborne]).toEqual([2, 1, 5, 8]);
    // duty = (60 - 2 x 5.56) / 80 = 0.611; 8 / 0.611 x 1.2 = 15.7 -> 16
    expect(p.duty).toBeCloseTo(0.611, 2);
    expect(p.total).toBe(16);
    expect(p.grounded).toBe(false);
  });
  it("never decreases when the front, wind or nearby places grow", () => {
    const base = planFleet(2, 10, null, 3).total;
    expect(planFleet(9, 10, null, 3).total).toBeGreaterThanOrEqual(base);
    expect(planFleet(2, 45, null, 3).total).toBeGreaterThanOrEqual(base);
    expect(planFleet(2, 10, null, 12).total).toBeGreaterThanOrEqual(base);
  });
  it("keeps at least one front drone and one head scout after an alert", () => {
    const p = planFleet(0, 0, null, 0);
    expect(p.perimeter).toBe(1);
    expect(p.head).toBe(1);
    expect(p.airborne).toBe(2);
  });
  it("grounds the fleet when gusts exceed the limit", () => {
    expect(planFleet(3, 30, FLEET.gust_limit_kmh + 1, 2).grounded).toBe(true);
  });
  it("assigns one task per airborne drone and stages upwind", () => {
    const ignition: [number, number] = [-7.82, 40.23];
    const p = planFleet(3, 25, null, 7);
    const ring: [number, number][] = [[-7.83, 40.22], [-7.81, 40.22], [-7.81, 40.24], [-7.83, 40.24], [-7.83, 40.22]];
    const places: [number, number][] = Array.from({ length: 7 }, (_, i) => [-7.8 - i * 0.01, 40.25]);
    expect(droneTasks(p, ignition, ring, ring, places)).toHaveLength(p.airborne);
    const base = stagingPoint(ignition, 0); // wind from the north -> staging to the north
    expect(base[1]).toBeGreaterThan(ignition[1]);
    expect(distanceKm(base, ignition)).toBeCloseTo(FLEET.staging_km, 1);
  });
});
