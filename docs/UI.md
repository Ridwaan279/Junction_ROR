# EmberWatch UI (Agent 1)

Branch: everything now lands on `main` (the user's decision, 15 Nov 2026). Agent 2
keeps its own branch and Agent 1 merges it into `main` at each checkpoint.

The home page (`/`) is the operations console in `web/components/ops/`. The previous
workspace is kept at `/workspace` and the 3D terrain explorer at `/terrain`.

## Decisions from the user

- Audience: hackathon judges. Keep information minimal; details open on click.
- The map opens in **3D** (satellite imagery on terrain) with no button press; a
  **2D** toggle is always available. Dark ops-room panels over the imagery.
- First glance in a fire: the fire and its predicted spread.
- Default mode is **live**. When nothing is burning, the screen says so plainly
  ("Sem incêndios ativos neste momento / No active fires right now"), with the
  time of the last check and its source. A button switches to the **Arganil 2025
  replay** (real data, docs/DEMO_DATA.md) and back.

## Screens

| Mode | What is on screen |
|---|---|
| Live | Portugal in 3D; status card (no active fires / N active fires / check failed), source and time of the last check, button to the replay |
| Replay | Simulated drones (staging points, routes draped on the terrain, drones moving along them) from the first frame that has a drone plan; "Model accuracy" panel comparing each forecast with the next observed GWIS perimeter (overlap on total and on new growth, with the reasons it over- or under-predicts). Fire on 3D terrain: modelled burned area (10/50/90 %), 6 h and 12 h forecast envelopes, dense smoke (≥150 µg/m³), villages named in verified reports (red evacuated, amber other mentions), ignition point; situation card (Fogos.pt status, modelled area and spread, wind, temperature, humidity, gusts, automatic-response steps marked observed/simulated, evacuated villages, facts and sources); news pop-ups at key moments; timeline with play/pause; legend |

Camera framing uses map padding, so the subject sits beside the panels on desktop and below them on phones. Rules the UI enforces: nothing is shown before it was public (frame = latest frame
not after the cursor; facts and villages filtered by `known_at` to the minute);
unverified facts are never shown; simulated steps are labelled.

## Map stack

- Imagery: Sentinel-2 cloudless 2016 by EOX (CC BY 4.0). Later EOX years are
  CC BY-NC-SA, so they are not the default. Override with `NEXT_PUBLIC_SATELLITE_TILES`.
- Terrain: Mapterhorn DEM (terrarium), exaggeration 1.45. Labels and thin roads
  from OpenFreeMap. Credits sit behind the (i) control.
- Fire, smoke and village layers are native MapLibre layers, so they drape on
  the terrain in 3D.

## Live status contract (for the Rust service, Agent 2)

`GET {NEXT_PUBLIC_API_BASE_URL}/api/live/status` returns:

```ts
type LiveStatus = {
  mode: "live";
  checked_at: string;                       // UTC, last completed check
  state: "no_active_fires" | "active_fires" | "check_failed" | "partial";
  statement: string | null;                 // verbatim page text that proves the state, e.g. "Sem registo de incêndios"
  statement_en: string | null;
  active: { id: string; place: string; position: [number, number] | null; status: string; url: string }[];
  sources: { name: string; url: string; via: string; request_id: string | null; captured_at: string; status: string; is_partial: boolean; credits: number | null }[];
};
```

`no_active_fires` is allowed only when a capture succeeded, was not partial and the
page positively states there are no incidents (the Fogos.pt list page shows
"Sem registo de incêndios"). Anything else is `check_failed` or `partial`. Until the
endpoint exists, the UI reads the real snapshot in
`web/public/fixtures/v2/live-status.json` and labels it as the last check with the
live service offline.

## Next UI steps (pending the user's further input)

- Swap the legacy drone plan for Agent 2's `fleet` (fixed stations, 3D smoke-avoiding routes with altitude) when delivered.
- Point the live card at `GET /api/live/status` once the Rust service serves it.
- Finer early frames (Agent 2, WORKPLAN 5.0b) will move "Drones launched" from 08:00 to minutes after the 05:08 report.
