# Agent 2 handoff

Branch: codex/agent-2-intelligence. Completion commit subject: I pushed agent 2.
Original base: 21e03ce862a53516f0b38e79aa7a5e6897638ca4; rebased integration base: 33bc89fb.
Published implementation checkpoint before rebase: 2262967cd3bd51f6509e7e0e2066af225b0d5efd.

Delivered: shared Context transport and credit accounting; bounded Rust capture-plan,
batch and private archive tools; original hashes; conservative quote/number/time
checks; honest live status; 20 split historical frames with half-hour early spacing;
three cited hypothetical stations; real regional terrain; tested fleet formula;
smoke-blocking 3D A* missions with return reserve and schematic ground transfers;
exposure probability/priority and clustered task points; deterministic explanations
and simulated cursor-time status reports. Gemini and the human benchmark are omitted
by the user's instruction. No independent accuracy certification is claimed.

## Integration

Switch Agent 1's replay index URL to /generated/arganil-early/index.json. No UI source
was edited by Agent 2. All advisor fields remain empty. Source articles stay private.

- GET /api/live/status: capture success and positive-empty status per source.
- GET /api/stations: hypothetical drone catalog when run_id is absent; the old
  run-scoped weather-station response remains available when run_id is supplied.
- POST /api/incidents/arganil-early/fleet?run_id=historical:arganil-early&at=RFC3339.
- GET /api/incidents/arganil-early/exposures with the same run_id and at query.

The last two routes reject live/unknown scopes and never select a future frame.
Additive fields: Route.flight_duration_s/ground_duration_s; exposure p_fire_6h,
priority, assumed_people_for_priority and automation_eligible; index stations and
airframe; source capture_succeeded, positively_empty and last_successful_capture_at.
Unknown capture times and unresolved exposure distances are null; UI types should
accept those nulls. Reports reuse automation first_report events, basis simulated.

## Validation and scope

See validation.json. Full Rust fmt/clippy/test passed in Linux; native Windows
Application Control blocked SQLx and selected executables, without policy changes.
The artifact test checks frame spacing, unchanged shared spread frames, evidence
and automation cursor bounds, source-vs-assumption attendance, terrain clearance,
smoke thresholds and flight endurance/return reserve. All historical spread
validation rows are unchanged. No web code changed, so web checks belong to Agent 1.

Run git diff --name-only 33bc89fb..HEAD; scope.txt records the output. Changes outside
the normal Agent 2 column are user-authorized documentation updates removing Gemini
and the human benchmark: README.md, docs/PLAN.md, docs/WORKPLAN.md, docs/DEMO_DATA.md.

## Environment and limits

CONTEXT_DEV_API_KEY; EMBERWATCH_LIVE_ENABLED; FOGOS_PUBLIC_PAGE_CHECKS_PASSED;
LIVE_STATUS_CACHE_SECONDS; CONTEXT_LIVE_HOURLY_CREDITS; FIRECAST_CONTEXT_MAX_CREDITS;
EMBERWATCH_GOLDEN_RAW_DIR (private originals for rebuilding); EMBERWATCH_GENERATED_DIR
(optional server replay directory). No Gemini key/model/network access is needed.
Context enrichment is bounded per run/incident/hour; uncertain usage stays frozen.
Article extraction is cached for one hour. The committed replay spends no credits.

Open integration items: Agent 1 switches the UI URL and reviews additive/null fields.
Live fleet planning is not substituted with historical outputs. Existing GeoNames
place matches retain attribution; a complete OSM municipality gazetteer is not
available, and ambiguous/unresolved names stay unmapped. Terrain/fuel, surface-smoke
and aircraft assumptions make this an experimental demonstration, not dispatch
guidance. Ground legs do not use a road network. Retrospective 2026 captures and
ERA5 cannot prove operational availability in 2025. Source reuse checks remain
recorded in docs/DATA_SOURCES.md. See the early-demo README for rebuild commands.
