# EmberWatch web

Next.js map workspace for mainland Portugal. Default data is explicitly synthetic.
Not suitable for emergency decisions or real drone dispatch.

Run `npm ci` then `npm run dev` from `web/`. Checks: `npm test`, `npm run lint`,
`npm run typecheck`, `npm run build`.

Set `NEXT_PUBLIC_API_BASE_URL` to the Rust service (for example http://localhost:8080).
Without it the bundled synthetic fixture powers a local replay. API errors preserve
last received data marked disconnected; they never imply zero incidents.

`NEXT_PUBLIC_MAP_STYLE_URL` optionally configures a licensed MapLibre style. The
fallback uses standard OpenStreetMap raster tiles with visible attribution for
local demonstration. Respect https://operations.osmfoundation.org/policies/tiles/;
no bulk/offline downloads and no production high-traffic deployment on that
public service. A labelled geographic fallback remains usable without WebGL.

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
