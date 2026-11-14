import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  applyEvent,
  parseSnapshot,
  safeUrl,
  displayTime,
  type StreamState,
} from "../lib/model";
import type { Snapshot, EventEnvelope } from "../lib/contracts";
const fixture = () =>
  JSON.parse(
    readFileSync(
      new URL("../public/snapshot.json", import.meta.url),
      "utf8",
    ).replace(/^\uFEFF/, ""),
  ) as Snapshot;
const message = (
  snapshot: Snapshot,
  type = "scenario.tick",
): EventEnvelope => ({
  id: "new-id",
  type,
  run_id: snapshot.scenario.run_id,
  scenario_id: snapshot.scenario.id,
  epoch: snapshot.scenario.epoch,
  event_time: snapshot.scenario.current_at,
  emitted_at: snapshot.scenario.current_at,
  payload: snapshot,
});
describe("serialized boundary", () => {
  it("loads the coordinator's synthetic snapshot without custom browser fusion", () => {
    expect(parseSnapshot(fixture()).scenario.mode).toBe("synthetic");
  });
  it("rejects mixed-run and invalid-coordinate payloads", () => {
    const mixed = fixture();
    mixed.evidence[0].run_id = "another-run";
    expect(() => parseSnapshot(mixed)).toThrow("mixed-run");
    const invalid = fixture();
    invalid.incidents[0].position = [200, 40];
    expect(() => parseSnapshot(invalid)).toThrow();
  });
  it("atomically replaces state on a tick or mission snapshot", () => {
    const snapshot = fixture(),
      state: StreamState = { snapshot, needsResync: false, seen: [] };
    const next = fixture();
    next.scenario.current_at = "2026-11-14T12:10:00Z";
    next.evidence = [];
    expect(applyEvent(state, message(next)).snapshot.evidence).toEqual([]);
    expect(applyEvent(state, message(next, "drone.plan")).snapshot).toBe(next);
  });
  it("refuses an old tick from the same epoch", () => {
    const old = fixture(),
      next = fixture();
    next.scenario.current_at = "2026-11-14T12:10:00Z";
    const state = { snapshot: next, needsResync: false, seen: [] };
    expect(applyEvent(state, message(old))).toBe(state);
  });
  it("blocks executable citation links", () => {
    expect(safeUrl("javascript:alert(1)")).toBeUndefined();
    expect(safeUrl("https://fogos.pt/pt")).toBe("https://fogos.pt/pt");
  });
  it("displays Lisbon summer time instead of the operating-system timezone", () => {
    expect(displayTime("2026-07-01T12:00:00Z", "en")).toBe("13:00");
    expect(displayTime(null, "pt")).toBe("Desconhecido");
  });
});
