"""Exercise a running local API; --restored checks a restart of the same state."""
import json
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

base = sys.argv[1].rstrip("/")
restored = "--restored" in sys.argv[2:]


def request(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    headers = {} if data is None else {"Content-Type": "application/json"}
    with urlopen(Request(base + path, data=data, headers=headers), timeout=3) as response:
        return json.load(response)


for attempt in range(30):
    try:
        health = request("/healthz")
        break
    except (URLError, TimeoutError, ConnectionError):
        if attempt == 29:
            raise
        time.sleep(0.2)

assert health["status"] == "ok"
assert health["storage"] == "file"
scenarios = request("/api/scenarios")
assert len(scenarios) == 1, "startup must not duplicate the seeded scenario"
run = scenarios[0]
assert run["spec"]["mode"] == "synthetic"
run_id = run["id"]
assert not run["playing"]

if restored:
    assert run["epoch"] >= 2, "restart must invalidate the active playback epoch"
    assert len(run["evidence"]) == 3
else:
    assert len(run["evidence"]) == 1
    assert len(run["weather"]) == 1
    incidents = request(f"/api/incidents?run_id={run_id}")
    assert incidents[0]["evidence_level"] == "reported"
    sources = request("/api/sources")
    assert all(not source["enabled"] for source in sources)
    assert next(source for source in sources if source["id"] == "fogos-context")["health"] == "needs_validation"
    try:
        request("/api/scenarios", run["spec"])
        raise AssertionError("operator-only mutation was accepted")
    except HTTPError as error:
        assert error.code == 401
    snapshot = request(f"/api/scenarios/{run_id}/seek", {"at": "2026-01-01T12:25:00Z"})
    assert snapshot["epoch"] == 1
    assert len(snapshot["evidence"]) == 3
    assert len(snapshot["weather"]) == 2
    assert snapshot["incidents"][0]["evidence_level"] == "satellite_supported"
    with urlopen(base + f"/api/events?run_id={run_id}", timeout=3) as stream:
        lines = []
        for _ in range(8):
            line = stream.readline()
            lines.append(line)
            if line == b"\n":
                break
        assert b"event: run.reset" in b"".join(lines)
    playing = request(f"/api/scenarios/{run_id}/play", {"speed": 60})
    assert playing["playing"]

print(json.dumps({
    "http_smoke": "restart_passed" if restored else "initial_passed",
    "storage": health["storage"],
    "mode": run["spec"]["mode"],
    "run_id": run_id,
}))
