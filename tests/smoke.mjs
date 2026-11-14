// Run against the local synthetic server: node tests/smoke.mjs
import assert from "node:assert/strict";
const base = process.env.EMBERWATCH_API_URL ?? "http://127.0.0.1:8080";
const run = "synthetic-portugal";
async function request(path, method = "GET", body) {
  const response = await fetch(base + path, {
    method,
    ...(body ? { headers: { "content-type": "application/json" }, body: JSON.stringify(body) } : {}),
    signal: AbortSignal.timeout(10000),
  });
  assert.equal(response.status, 200, `${method} ${path}`);
  return response.json();
}
const command = (name, body) => request(`/api/scenarios/portugal-synthetic/${name}?run_id=${run}`, "POST", body);
assert.equal((await request("/healthz")).status, "ok");
assert.equal((await fetch(base + "/api/snapshot")).status, 400);
assert.equal((await fetch(base + "/api/snapshot?run_id=other")).status, 404);
let snapshot = await command("reset");
assert.equal(snapshot.evidence.length, 1);
snapshot = await request(`/api/incidents/incident-training-1/dispatch?run_id=${run}`, "POST");
assert.equal(snapshot.forecasts.length, 1);
assert.equal(snapshot.forecasts[0].illustrative, true);
assert.equal(snapshot.drone_plans[0].simulated, true);
assert.ok(snapshot.drone_plans[0].route.length > 1);
assert.ok(snapshot.briefings[0].citations.includes("https://example.org/emberwatch/synthetic/report-1"));
snapshot = await command("seek", { at: "2026-11-14T12:20:00Z" });
assert.equal(snapshot.evidence.length, 2);
assert.equal(snapshot.incidents.length, 1);
assert.equal(snapshot.forecasts.length, 0);
snapshot = await command("reset");
assert.equal(snapshot.evidence.length, 1);
assert.equal(snapshot.drone_plans.length, 0);
await command("play", { speed: 60 });
const stream = await fetch(base + `/api/events?run_id=${run}`, { signal: AbortSignal.timeout(5000) });
assert.ok(stream.headers.get("content-type").includes("text/event-stream"));
const reader = stream.body.getReader();
const decoder = new TextDecoder();
let buffer = "", sawReset = false, sawTick = false;
try {
  while (!sawTick) {
    const { value, done } = await reader.read();
    assert.equal(done, false);
    buffer += decoder.decode(value, { stream: true });
    let boundary;
    while ((boundary = buffer.indexOf("\n\n")) >= 0) {
      const block = buffer.slice(0, boundary); buffer = buffer.slice(boundary + 2);
      const data = block.match(/^data: (.+)$/m)?.[1];
      if (!data) continue;
      const event = JSON.parse(data);
      assert.equal(event.run_id, run);
      if (event.type === "run.reset") sawReset = true;
      if (event.type === "scenario.tick") {
        assert.equal(sawReset, true);
        assert.ok(Date.parse(event.payload.scenario.current_at) > Date.parse("2026-11-14T12:00:00Z"));
        sawTick = true;
      }
    }
  }
} finally { await reader.cancel(); await command("pause"); await command("reset"); }
console.log("PASS: scoped API, simulated response, backward seek/reset, and initial snapshot + live SSE tick");
