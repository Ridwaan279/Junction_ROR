# Deploying Brasa Scout (EmberWatch)

Two parts. The website alone is a complete demo (3D map, Arganil replay, drones,
pins, search and rescue, Data and Methods). The Rust service adds live Fogos.pt
checks through Context.dev.

## 1. Website (Next.js) on Vercel

1. vercel.com → Add New → Project → import `Ridwaan279/Junction_ROR` (private repos need the Vercel GitHub app).
2. **Root Directory: `web`**. Framework preset: Next.js. Build and output settings: defaults.
3. Environment variable (only if the service in part 2 is running):
   `NEXT_PUBLIC_API_BASE_URL = https://<your-service-host>` (no trailing slash).
   It is baked in at build time, so redeploy after changing it.
4. Deploy. Every push to `main` redeploys.

Without `NEXT_PUBLIC_API_BASE_URL` the live card says "Live service not connected"
and shows the last saved check with its age; it never presents it as live.

## 2. Rust service (Docker) on Fly.io, Render or Railway

Image: `infra/Dockerfile`, build context = repository root. It listens on
`0.0.0.0:8080` and serves `/healthz`.

Set on the host (secrets in the host's secret store, never in Git or the website):

| Variable | Value |
|---|---|
| `CONTEXT_DEV_API_KEY` | secret |
| `EMBERWATCH_LIVE_ENABLED` | `1` |
| `FOGOS_PUBLIC_PAGE_CHECKS_PASSED` | `1` |
| `CONTEXT_LIVE_HOURLY_CREDITS` | e.g. `80` (one check costs about 1–6 credits; results are cached 5 min) |
| `WEB_ORIGIN` | the website's exact origin, e.g. `https://emberwatch.vercel.app` |

Fly.io example, from the repository root:

```bash
fly launch --no-deploy --dockerfile infra/Dockerfile --internal-port 8080
fly secrets set CONTEXT_DEV_API_KEY=... EMBERWATCH_LIVE_ENABLED=1 FOGOS_PUBLIC_PAGE_CHECKS_PASSED=1 CONTEXT_LIVE_HOURLY_CREDITS=80 WEB_ORIGIN=https://<site>.vercel.app
fly deploy
```

Render: New → Web Service → Docker, Dockerfile path `infra/Dockerfile`, port 8080,
health check `/healthz`, same variables.

Check: `https://<service>/api/live/status` returns JSON with a current
`checked_at`. Then set `NEXT_PUBLIC_API_BASE_URL` on Vercel and redeploy.

State is a file in the container (`/app/.state`), reset on redeploy; set
`DATABASE_URL` to a PostgreSQL/PostGIS database to keep it.

## Before sharing publicly

The site shows short quotes with links to Portuguese news sites and Fogos.pt.
Reuse rights for public redistribution are not confirmed (AGENTS.md,
docs/DATA_SOURCES.md); for judging, prefer a private or password-protected
deployment (Vercel → Settings → Deployment Protection). Full article text stays
out of the repository and the deployment.
