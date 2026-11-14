# EmberWatch web

Next.js map workspace for mainland Portugal. Default data is explicitly synthetic.
Not suitable for emergency decisions or real drone dispatch.

Run `npm ci` then `npm run dev` from `web/`. Checks: `npm test`, `npm run lint`,
`npm run typecheck`, `npm run build`.

Set `NEXT_PUBLIC_API_BASE_URL` to the Rust service (for example http://localhost:8080).
Without it the bundled synthetic fixture powers a local replay. API errors preserve
last received data marked disconnected; they never imply zero incidents.

Open `/terrain` for a full-screen mainland Portugal terrain explorer. It includes
2D/3D switching, shaded relief, generated elevation contours, tilt, vertical
exaggeration, orbit, north reset, zoom, and shareable camera links.
Portuguese/English controls are available from the header. Search covers
eight curated places (accent-insensitive) or `latitude, longitude` within the
mainland bounding box; it is not a general address geocoder. The workspace also
uses this terrain basemap and links to the explorer. Incident/forecast overlays
stay in the workspace's 2D view.

Choose **Explore 3D buildings** to fly into Lisboa at building scale, or select
Porto/Lisboa while in 3D. The **3D buildings** switch controls extrusion; flat
footprints remain in 2D. Click a building for its source and height explanation.
Place labels are optional to keep the city geometry clear; street names remain.
City presets use 1× terrain exaggeration. Building heights are in metres and
are not multiplied by the terrain exaggeration control.

Building footprints come from OpenStreetMap through OpenFreeMap/OpenMapTiles.
They follow real mapped outlines, with coverage, age and geometry accuracy varying
by location. OpenMapTiles `render_height`/`render_min_height` are approximations
that can come from mapped heights, estimated floor counts or provider defaults.
This tile schema does not establish which was used for each building. The
inspector therefore labels them provider approximations and keeps **verified
height unknown**. If render height is absent or invalid, the explicit display
fallback is 5 m. Roof and façade colours are illustrative. No photographic
textures, surveyed sizes, current-condition verification, or complete coverage
are claimed. See [building sources and the photorealistic path](../docs/BUILDINGS.md).

The default map needs **no API key or account**. MapLibre renders
[OpenFreeMap's Liberty vector style](https://openfreemap.org/quick_start/) with
terrain colours and [Mapterhorn elevation tiles](https://mapterhorn.com/data-access/).
Both public services currently offer free access. Keep the visible OpenFreeMap,
OpenMapTiles, OpenStreetMap and
[Mapterhorn/data-provider attribution](https://mapterhorn.com/attribution/).
Public endpoints have no project-specific availability guarantee; assess usage,
licences and hosting capacity before production deployment. Do not bulk-download
tiles. Terrain is elevation-derived geometry, not photorealistic buildings or a
survey. The elevation readout is approximate and the exaggeration control changes
the display only. Camera presets are approximate.

Map rendering requires internet access and WebGL. The incident workspace retains
its labelled geographic fallback without WebGL; the dedicated explorer shows a
retry message when it cannot initialize. No map imagery or DEM is bundled for
offline use. Next.js emits MapLibre's worker asset through an explicit worker URL.
`maplibre-contour` generates contours in a separate worker and shares its DEM cache
with terrain/hillshade sources.

`NEXT_PUBLIC_MAP_STYLE_URL` optionally configures a licensed MapLibre style; its
source attribution is preserved. Google imagery/photorealistic 3D is not integrated.
That would need a Google Maps Platform project, an appropriate enabled API and a
restricted browser key, with production billing configured under
[Google's current setup rules](https://developers.google.com/maps/documentation/javascript/get-api-key).
No Gemini key is needed to render this geographically grounded map.

The browser displays supplied forecast/drone outputs; it does not compute them.

The public environment variables are embedded at build time: set them before
running `npm run build` (or before `npm run dev`). Do not put provider secrets in
`NEXT_PUBLIC_*` variables.

`npm run test:e2e` runs the Chrome click-through and mobile fallback checks against
a production build on port 3001. It uses the machine's installed Chrome; install
Chrome or adjust `playwright.config.ts` to an installed Playwright browser.
Screenshots are saved under ignored `web/artifacts/`. Unit tests cover replay
epoch isolation, availability-time gating, cross-run validation, citations, and
Lisbon daylight-saving time.

The offline replay filters supplied observations by availability time and does
not infer new incident status or corroboration. Simulated mission generation
requires the Rust service. The mission button is enabled only for synthetic runs.
Map FPS is sampled from actual render events while the map camera moves; it stays
"not measured" when WebGL is unavailable. The fallback outline is approximate and
does not provide terrain, licensed airspace, or navigation geometry.

For the integrated app, run `npm run test:e2e:online` while the web app is already
running on http://127.0.0.1:3000 with API http://127.0.0.1:8080. This opt-in check
resets the synthetic scenario, generates a simulated mission, reloads to
resynchronize the event stream, then resets and verifies all mission outputs clear.
Override URLs with `EMBERWATCH_ONLINE_URL` and `EMBERWATCH_API_TEST_URL`.
