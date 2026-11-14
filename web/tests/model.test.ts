import { describe, expect, it } from "vitest";
import {
  applyEvent,
  coverageMessage,
  replayAt,
  type StreamState,
} from "../lib/model";
import type { EventEnvelope, Snapshot } from "../lib/contracts";
const start = "2026-01-01T10:00:00Z",
  later = "2026-01-01T10:10:00Z";
const snapshot = (): Snapshot => ({
  scenario: {
    id: "demo",
    run_id: "run-a",
    name: "Synthetic",
    mode: "synthetic",
    start_at: start,
    end_at: later,
    current_at: later,
    playing: false,
    speed: 60,
    seed: 7,
    epoch: 0,
    description: "Synthetic",
  },
  incidents: [
    {
      id: "fire-a",
      run_id: "run-a",
      mode: "synthetic",
      title: "Synthetic report",
      location_name: "Aveiro",
      position: [-8.4, 40.7],
      uncertainty_m: 500,
      status: "reported",
      evidence_level: "reported",
      evidence_ids: ["e1"],
      origin_groups: ["fixture"],
      first_event_at: start,
      updated_at: later,
    },
  ],
  evidence: [
    {
      id: "e1",
      run_id: "run-a",
      mode: "synthetic",
      provenance: {
        source_id: "fixture",
        source_url: "https://example.org",
        origin_group: "fixture",
        external_id: null,
        event_at: start,
        published_at: null,
        captured_at: later,
        available_at: later,
        availability_basis: "synthetic",
        evidence_hash: "fixture-1",
        quality_flags: [],
      },
      position: [-8.4, 40.7],
      uncertainty_m: 500,
      location_name: "Aveiro",
      municipality: "Aveiro",
      kind: "report",
      raw_status: null,
      title: "Synthetic report",
      body: null,
      attributes: {},
    },
  ],
  weather: [],
  sources: [
    {
      id: "fogos",
      name: "Fogos.pt",
      source_url: "https://fogos.pt/pt",
      via: "Context.dev",
      origin_group: "anepc",
      enabled: false,
      health: "failed",
      coverage: {
        complete: false,
        explicit_empty: false,
        scope: "unknown",
        notes: [],
      },
      last_success_at: null,
      captured_at: null,
      data_at: null,
      attribution: "Fogos.pt",
      notes: [],
    },
  ],
  forecasts: [],
  drone_plans: [],
  briefings: [],
});
const state = (): StreamState => ({
  snapshot: snapshot(),
  needsResync: false,
  seen: [],
});
const event = (extra: Partial<EventEnvelope> = {}): EventEnvelope => ({
  id: "1",
  type: "run.reset",
  run_id: "run-a",
  scenario_id: "demo",
  epoch: 1,
  event_time: start,
  emitted_at: start,
  payload: {},
  ...extra,
});
describe("replay isolation", () => {
  it("reset removes every stale data layer before a resync", () => {
    const next = applyEvent(state(), event());
    expect(next.snapshot.incidents).toEqual([]);
    expect(next.snapshot.evidence).toEqual([]);
    expect(next.snapshot.forecasts).toEqual([]);
    expect(next.snapshot.drone_plans).toEqual([]);
    expect(next.snapshot.scenario.epoch).toBe(1);
    expect(next.needsResync).toBe(true);
  });
  it("ignores another run and late events from a previous epoch", () => {
    const current = state();
    current.snapshot.scenario.epoch = 2;
    expect(applyEvent(current, event({ run_id: "run-b" }))).toBe(current);
    expect(
      applyEvent(current, event({ epoch: 1, type: "incident.upsert" })),
    ).toBe(current);
  });
  it("resyncs instead of mixing an unannounced new epoch", () => {
    const next = applyEvent(state(), event({ type: "forecast.ready" }));
    expect(next.needsResync).toBe(true);
    expect(next.snapshot.incidents).toEqual([]);
  });
  it("deduplicates a delivered event", () => {
    const current = state();
    current.seen = ["duplicate"];
    expect(
      applyEvent(
        current,
        event({ id: "duplicate", epoch: 0, type: "evidence.new" }),
      ),
    ).toBe(current);
  });
  it("never releases an observation before its availability time on seek", () => {
    const rewound = replayAt(snapshot(), start, 2);
    expect(rewound.evidence).toEqual([]);
    expect(rewound.incidents).toEqual([]);
    expect(rewound.scenario.epoch).toBe(2);
    expect(replayAt(snapshot(), later, 3).evidence).toHaveLength(1);
  });
  it("failed and missing source coverage cannot display all clear", () => {
    expect(coverageMessage(snapshot())).toBe("unknown");
    const noSources = snapshot();
    noSources.sources = [];
    expect(coverageMessage(noSources)).toBe("unknown");
  });
});
