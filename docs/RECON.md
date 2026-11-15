# Simulated reconnaissance camera slice

Implemented and locally checked on 15 November 2026. No commit, push or deployment.

## What runs

`/recon` adds an operator-assigned mission and incident card to the existing web
app. It uses the existing Rust `/api/snapshot` synthetic event and an additive
`/api/recon` API. The existing Snapshot schema and SSE envelope are unchanged.
There is a navigation link from the operations screen.

The verified flight uses the **existing Gazebo kinematic stand-in** from
`ros2/src/emberwatch_drone/worlds/emberwatch_scene.sdf`, with a mounted oblique RGB
camera added to a generated copy. Frames are rendered by Gazebo ogre2, bridged
through `ros_gz_bridge` into ROS `sensor_msgs/Image`, encoded as JPEG and uploaded
to the Rust backend. The browser retrieves the backend's frame bytes through a
server-side Next.js proxy. No mock image, screenshot substitute, external camera
feed, new service dependency or image generator is used.

Mission states are `assigned → travelling → observing → returning → complete`,
with a separate `failed` state. The operator assigns the mission and can request
return. The drone captures its current odometry as its start/return position;
it climbs, flies to an oblique observation station, observes, returns and descends.
No water command is issued. This is a bounded representative scene corridor,
**not PX4 flight dynamics or a geographic obstacle-avoidance planner**.

The existing four-drone PX4 containment implementation was inspected, preserved
and never commanded by this integration. At inspection it was flying routes
and entering simulated spray phases in `GZ_PARTITION=emberwatch_px4`. Its x500s
had no published camera image topics. Its generated Arganil incident differs
from the running backend's synthetic Sever do Vouga incident. These identities
were not merged or relabelled. The existing run's later `summary.json` records
all four drones `DONE`; that is evidence from the existing controller, not a new
PX4 reconnaissance verification.

**Remaining integration:** converting that four-drone PX4 controller to recon,
adding mounted PX4 cameras and connecting matching geographic backend events is
not implemented or tested here. The reliable camera proof uses one existing
kinematic drone in an isolated world. The interface already permits separate
task/drone IDs; the delivered page tasks `ember_drone` only.

## Run locally

Keep the original backend on 8080 and any existing PX4/Gazebo sessions running.
The following commands use a second instance of the same Rust backend on 8081.
Run each long-lived command in its own terminal, from the repository root:

```sh
# Existing pinned Rust toolchain; no keys required and no upstream ingestion.
PATH="$HOME/.cargo/bin:$PATH" BIND_ADDRESS=127.0.0.1:8081 \
  STATE_PATH=.state/recon-platform.json FIRECAST_AUTO=false FIRECAST_NETWORK=false \
  cargo run -p emberwatch-server

cd web
npm ci --no-audit --no-fund
RECON_API_BASE_URL=http://127.0.0.1:8081 npm run dev

# Back at the repository root, in another terminal:
bash ros2/scripts/run_recon_demo.sh --api http://127.0.0.1:8081 --observe-s 45
```

Open `http://127.0.0.1:3000/recon`, choose PT/EN and click **Assign simulated
reconnaissance**. An unassigned simulator holds zero velocity. The default
observation duration is 15 wall seconds; `--observe-s` accepts up to 60.
`--exit-when-done` exits after the final backend observation is acknowledged.
Without it, the demo stays available for another operator assignment.

The launcher creates a unique Gazebo partition, pins transport to loopback and
uses ROS domain 94. Override `RECON_ROS_DOMAIN_ID` for a different domain and
`RECON_GZ_PARTITION` for a reproducible partition name. A per-domain local lock
prevents two recon demos from sharing ROS camera/control topics. No PX4 process,
XRCE agent, UDP 8888 or existing simulator session is started or stopped. Cleanup
signals only process groups created by this launcher.

The existing controller's world and launch script are not changed. Generated
worlds/logs and verification images stay under ignored `ros2/generated/recon/`.
Stop the recon launcher with Ctrl-C. Stop the new web/backend terminals separately.
`RECON_API_BASE_URL` is server-side only; no credentials enter browser code.

## What the card means

| Field | Actual origin / limitation |
|---|---|
| Reported fire longitude/latitude | Existing backend incident, `[longitude, latitude]`. Not the scene camera's coordinates. |
| Reported time | Incident `first_event_at`; remains unknown for the bundled event. |
| Event update/source/capture/quality | Copied backend incident and evidence provenance, including origin group and quality flags. The example.org link is explicitly synthetic. |
| Image | Actual 640×480 Gazebo RGB camera render, JPEG encoded locally. The representative scene is not a reconstruction of Sever do Vouga or Arganil. |
| Frame capture UTC | Wall-clock timestamp when the bridge receives the image; **not** a fabricated historical camera timestamp. Display uses `Europe/Lisbon`. |
| Simulator timestamp | ROS image header stamp in simulation seconds, shown separately from UTC. |
| Drone position at capture | Nearest odometry sample within 0.2 simulation seconds, local ENU metres at the drone base link. Null if no matching sample. Camera lens offset is documented in generated SDF. |
| Geographic drone position | Unknown/null: the representative scene is not georeferenced. |
| Extent, intensity and wind on scene | Unknown. This scene has visual fire/smoke effects, not a measured extent/intensity/wind model. No acreage, spread-rate or severity claim. |
| Observation method | `simulator_state`, never image-based detection. Visual smoke is not a sensed hazard. |

The original Arganil FireModel and containment/water simulation remain separate.
No spot-fire detector, ember attribution, payload claim or suppression-effectiveness
claim was added. Recon has no connection to Portuguese responders or real aircraft.

## Additive web/backend contract

Use `?run_id=synthetic-portugal` with all routes. Recon currently accepts only
synthetic events on the original `/api` surface, not `/platform/ui` live runs.

| Method / path | Meaning |
|---|---|
| `POST /api/recon/tasks` | `{ "incident_id": "incident-training-1", "drone_id": "ember_drone" }`; operator assignment. Retries return the existing active assignment. |
| `GET /api/recon/tasks` | Current run/epoch tasks, observation/frame metadata and derived freshness flags. |
| `POST /api/recon/tasks/{id}/return` | Requests return; does not report a completed return before simulator confirmation. |
| `POST /api/recon/tasks/{id}/observations` | Simulation flag, increasing sequence, mission state, UTC `observed_at`, nullable sim time/positions, camera status/topic, method and note. |
| `POST /api/recon/tasks/{id}/frame?run_id=…&sequence=N` | Raw JPEG bytes matching the current fresh observation. |
| `GET /api/recon/tasks/{id}/frame?run_id=…&sequence=N` | Exact retained frame revision, `image/jpeg`, `Cache-Control: no-store`. A replaced revision returns 404; refetch task metadata. |

The task includes the full incident and its report provenance. Observation fields
are defined by the Rust DTO in `crates/server/src/recon_api.rs` and the matching
browser type in `web/lib/recon.ts`. The ROS adapter publishes its acknowledged
observation on `/emberwatch/recon/status` without changing existing mission topics.

New SSE types are `recon.assigned`, `recon.return_requested`, `recon.observation`
and `recon.frame`, on the existing `/api/events` stream. They keep `id`, `type`,
`run_id`, `scenario_id`, `epoch`, `event_time`, `emitted_at`, `payload`. Payload is
the task view; image bytes travel separately. Existing named snapshot events
remain unchanged. The new page polls current metadata every 1.5 seconds, so a
reconnection does not require replaying frame events.

Storage is explicitly ephemeral: at most 16 tasks and one ≤512 KiB JPEG per task.
Server restart loses recon state. Seek/reset invalidates the old epoch, including
frame access and updates, and the adapter holds zero velocity. The new ingest
routes trust a local simulator producer; they are not a remotely authenticated
camera attestation service. The demo binds to loopback and is not deployed.

## Failure behaviour

- Camera absent: no frame/placeholder image is fabricated. Camera status is
  `no_camera`; missing fields stay null. `--no-camera` exercises this case.
- Frame/telemetry older than 10 wall seconds: visible stale labels. A retained
  image is explicitly marked as an old view; freshness also ages locally if the
  backend becomes unavailable. Replaced-image 404s show camera unavailable.
- Stale event: `updated_at` older than 15 **scenario minutes** at assignment is
  flagged and the adapter refuses travel. Synthetic event age is evaluated using
  the scenario clock; camera freshness uses wall time.
- Missing/out-of-Portugal target or non-synthetic input: rejected/refused with
  `unreachable_target`/`no_route`; no straight-line geographic fallback.
- `--force-no-route`: explicit test-only corridor blockage; no flight is launched.
- No initial odometry (15 seconds), odometry stale (1 second), phase timeout
  (90 seconds), or backend unavailable (8 seconds): failure and zero velocity.
- Backend rejects observations more than 30 seconds old, over 5 seconds in the
  future, out of order, in another run/epoch, or making unsupported detection or
  geographic-position claims. Mission transitions are checked.

## Verification evidence

Commands used, from the repository root unless indicated:

```sh
PATH="$HOME/.cargo/bin:$PATH" cargo fmt --all -- --check
PATH="$HOME/.cargo/bin:$PATH" cargo clippy --workspace --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo test --workspace
PATH="$HOME/.cargo/bin:$PATH" cargo test -p emberwatch-server --test recon
python3 -m pytest -c ros2/pytest.ini
bash -n ros2/scripts/run_recon_demo.sh
cd web
npm test
npm run lint
npm run typecheck
npm run build
cd ..
node tests/recon-e2e.mjs
```

The real browser check performs no request interception: it assigns through the
UI, observes all five mission states, decodes a 640×480 camera frame in Chrome,
checks visible pixel variation, retains the actual JPEG/task metadata, requests
return through the UI and waits for completion. It captures desktop/mobile views
and checks browser exceptions. The first overhead-camera attempt produced a
uniform grey view above the visual plume; visual inspection caught it, the
station/camera were changed to an oblique view, and the scene-detail check was
added. The final retained image shows the fire, smoke and trees.

Artifacts: `ros2/generated/recon/verification/{result.json,observing.jpg,
observing-task.json,observing-desktop.png,observing-mobile.png}`. They are actual
local simulator/browser output, not mock frames. Test HTTP bodies in Rust are
transport fixtures; that test does not claim to decode a Gazebo image.

For the real failure check, stop the recon launcher after a completed camera
run, start `node tests/recon-failures.mjs`, then while it waits start:

```sh
bash ros2/scripts/run_recon_demo.sh --api http://127.0.0.1:8081 \
  --force-no-route --no-camera --exit-when-done
```

This checks stale frame/telemetry, an assignment with no mock camera image, and
a simulator-confirmed no-route failure. Artifacts are `stale.png`, `no-camera.png`,
`no-route.png` and `failures.json` in the same verification directory.

Unit/integration tests cover unknown/missing targets, stale input/odometry,
operator recall, complete return, provenance, mission transitions, sequence
ordering, frame binding, existing SSE-envelope compatibility, run isolation and
seek/reset invalidation. The full Rust run has optional ignored database tests
and an existing example-binary filename collision warning (`probe`); no database
or real upstream integration is claimed.

The existing Playwright workspace tests initially failed because they navigated
to `/`, which already renders the newer operations console. The legacy fixture
workspace is mounted at `/workspace`. Only those two navigation paths were
corrected; their original assertions were retained.

Launcher lifecycle validation additionally checked SIGTERM cleanup of all three
owned process groups and refusal of a second recon launcher on the same ROS
domain. Evidence: `verification/lifecycle.json`. No pre-existing processes were
included in those groups.
