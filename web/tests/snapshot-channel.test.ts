import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { SnapshotChannel } from "../lib/snapshot-channel";
import type { Snapshot, EventEnvelope } from "../lib/contracts";
const fixture = () =>
  JSON.parse(
    readFileSync(new URL("../public/snapshot.json", import.meta.url), "utf8"),
  ) as Snapshot;
function event(
  snapshot: Snapshot,
  type: string,
  id = "event-1",
): EventEnvelope {
  return {
    id,
    type,
    run_id: snapshot.scenario.run_id,
    scenario_id: snapshot.scenario.id,
    epoch: snapshot.scenario.epoch,
    event_time: snapshot.scenario.current_at,
    emitted_at: snapshot.scenario.current_at,
    payload: snapshot,
  };
}
describe("HTTP and SSE snapshot ordering", () => {
  it("does not move backward when a reconnect snapshot finishes after a newer tick", async () => {
    const original = fixture(),
      channel = new SnapshotChannel(original),
      ticket = channel.beginRequest();
    let resolve!: (snapshot: Snapshot) => void;
    const pending = new Promise<Snapshot>((done) => {
      resolve = done;
    }).then((snapshot) => channel.acceptHttp(snapshot, ticket));
    const newer = fixture();
    newer.scenario.current_at = "2026-11-14T12:10:00Z";
    channel.acceptStream(event(newer, "scenario.tick"));
    resolve(original);
    expect(await pending).toBeNull();
    expect(channel.state?.snapshot.scenario.current_at).toBe(
      newer.scenario.current_at,
    );
  });
  it("retains paused mission outputs when an equal-time HTTP snapshot returns later", async () => {
    const original = fixture(),
      channel = new SnapshotChannel(original),
      ticket = channel.beginRequest();
    const mission = fixture();
    mission.drone_plans = [
      {
        id: "new-plan",
        run_id: mission.scenario.run_id,
        incident_id: mission.incidents[0].id,
        epoch: 0,
        generated_at: mission.scenario.current_at,
        simulated: true,
        route: [[-8.4, 40.7]],
        return_route: [[-8.4, 40.7]],
        distance_m: 1000,
        duration_seconds: 120,
        reserve_pct: 30,
        reasons: ["Synthetic test"],
      },
    ];
    channel.acceptStream(event(mission, "drone.plan"));
    expect(channel.acceptHttp(original, ticket)).toBeNull();
    expect(channel.state?.snapshot.drone_plans[0].id).toBe("new-plan");
  });
  it("still accepts the latest HTTP result when no stream snapshot intervenes", () => {
    const original = fixture(),
      channel = new SnapshotChannel(original);
    const old = channel.beginRequest(),
      latest = channel.beginRequest();
    expect(channel.acceptHttp(original, old)).toBeNull();
    expect(channel.acceptHttp(original, latest)?.snapshot).toBe(original);
  });
  it("does not invalidate requests on ignored foreign or old stream messages", () => {
    const original = fixture(),
      channel = new SnapshotChannel(original),
      ticket = channel.beginRequest();
    const foreign = fixture();
    foreign.scenario.run_id = "foreign";
    expect(channel.acceptStream(event(foreign, "scenario.tick"))).toBeNull();
    expect(channel.acceptHttp(original, ticket)).not.toBeNull();
  });
});
