"""Build the early-stage Arganil/Piódão demo replay (13–14 Aug 2025) from real inputs only.

Inputs (all committed):
  context_dev/pages.json, search_results.json  Context.dev captures (facts + short quotes, no full text)
  context_dev/places.json                      GeoNames coordinates for places named in the facts
  ../../historical/arganil-piodao-2025/dataset.json  ERA5 weather, DEM-backed domain
  ../../../web/public/historical/arganil-piodao-2025.json  firecast model frames (3-hourly)
Output: web/public/fixtures/v2/arganil-early/{index.json, frames/NN.json}

Run from the repository root:  python3 -I data/demo/arganil-early/build.py
"""
import json, math, os, re, sys
from datetime import datetime, timedelta, timezone

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(ROOT, "..", "..", ".."))
OUT = os.path.join(REPO, "web", "public", "fixtures", "v2", "arganil-early")
WINDOW = ("2025-08-13T04:00:00Z", "2025-08-14T22:00:00Z")
LISBON_SUMMER = timedelta(hours=1)  # WEST (UTC+1) applies to every date in this window.
UTC = timezone.utc

def load(*p):
    return json.load(open(os.path.join(*p), encoding="utf-8"))

def iso(dt):
    return dt.astimezone(UTC).strftime("%Y-%m-%dT%H:%M:%SZ") if dt else None

def parse_iso(s):
    if not s:
        return None
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", s):
        return None  # date only: no time information
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(UTC)

MONTHS = {"jan": 1, "fev": 2, "mar": 3, "abr": 4, "mai": 5, "jun": 6, "jul": 7, "ago": 8, "set": 9, "out": 10, "nov": 11, "dez": 12}

def parse_printed(text):
    """Portuguese printed dates, e.g. '13 de agosto de 2025 às 13 h31', 'Agosto 13, 2025 . 11:36'.
    Returns (date, utc datetime or None when no time printed)."""
    if not text:
        return None, None
    t = text.lower()
    m = re.search(r"(\d{1,2})\s*(?:de\s+)?([a-zç]{3})[a-zç]*\.?,?\s*(?:de\s+)?(\d{4})", t) or None
    if m:
        day, mon, year = int(m.group(1)), MONTHS.get(m.group(2)[:3]), int(m.group(3))
    else:
        m = re.search(r"([a-zç]{3})[a-zç]*\s+(\d{1,2}),?\s*(\d{4})", t) or re.search(r"(\d{1,2})-(\d{1,2})-(\d{4})", t)
        if not m:
            return None, None
        if m.group(1).isdigit():
            day, mon, year = int(m.group(1)), int(m.group(2)), int(m.group(3))
        else:
            mon, day, year = MONTHS.get(m.group(1)[:3]), int(m.group(2)), int(m.group(3))
    if not mon:
        return None, None
    date = datetime(year, mon, day, tzinfo=UTC)
    rest = t[m.end():]
    hm = re.search(r"(\d{1,2})\s*(?::|h)\s*(\d{2})", rest)
    if not hm:
        return date, None
    local = date.replace(hour=int(hm.group(1)), minute=int(hm.group(2)))
    return date, local - LISBON_SUMMER

def url_date(url):
    m = re.search(r"(20\d{2})[/-](\d{2})[/-](\d{2})", url)
    return datetime(int(m.group(1)), int(m.group(2)), int(m.group(3)), tzinfo=UTC) if m else None

def end_of_local_day(date):
    return date.replace(hour=23, minute=59, second=59) - LISBON_SUMMER

def mentioned_times(page, date):
    """Clock times stated inside facts on the publication date ('pelas 16h15', 'às 17h', '11h')."""
    out = []
    for f in page["facts"]:
        for text in (f.get("time_text") or "", f["quote"]):
            for h, mnt in re.findall(r"\b(\d{1,2})\s*h\s*(\d{2})?\b", text.lower()):
                if int(h) < 24:
                    out.append(date.replace(hour=int(h), minute=int(mnt or 0)) - LISBON_SUMMER)
    return out

def resolve_times(page):
    """known_at = earliest time the captured content can be proven public (never earlier)."""
    flags = []
    meta_pub, meta_mod = parse_iso(page["meta_published"]), parse_iso(page["meta_modified"])
    pdate, printed = parse_printed(page["printed_published"])
    if page["printed_published"] and not page["printed_published_in_page"]:
        printed, pdate, flags = None, None, flags + ["printed_date_not_found_in_page"]
    published, basis = None, None
    if meta_pub and printed:
        if abs((meta_pub - printed).total_seconds()) > 300:
            flags.append(f"printed {iso(printed)} and metadata {iso(meta_pub)} disagree; later one used")
        published, basis = max(meta_pub, printed), "page metadata + printed time"
    elif meta_pub or printed:
        published, basis = meta_pub or printed, "page metadata" if meta_pub else "printed time (Europe/Lisbon)"
    date = pdate or (meta_pub and meta_pub.replace(hour=0, minute=0, second=0)) or url_date(page["url"])
    if not published and date:
        published, basis = end_of_local_day(date), "date only; end of that local day"
        flags.append("no publication time: revealed at end of its date")
    known = [t for t in (published, meta_mod) if t]
    if date:
        later = [t for t in mentioned_times(page, date) if published and t > published]
        if later:
            known.append(max(later))
            flags.append("content mentions later clock times; revealed at the latest mentioned time")
    return {"published_at": iso(published), "modified_at": iso(meta_mod), "time_basis": basis,
            "known_at": iso(max(known)) if known else None, "time_flags": flags}

EVENT_WORDS = re.compile(r"\b(festa|festas|feira|romaria|festival|arraial|procissão|concerto|evento|espetáculo)\b")
WEEKDAYS = {"segunda-feira": 0, "terça-feira": 1, "quarta-feira": 2, "quinta-feira": 3, "sexta-feira": 4, "sábado": 5, "domingo": 6}

def km(a, b):
    p = math.pi / 180
    return 6371 * 2 * math.asin(math.sqrt(math.sin((b[1] - a[1]) * p / 2) ** 2 + math.cos(a[1] * p) * math.cos(b[1] * p) * math.sin((b[0] - a[0]) * p / 2) ** 2))

def in_ring(pt, ring):
    x, y, inside = pt[0], pt[1], False
    for i in range(len(ring)):
        (x1, y1), (x2, y2) = ring[i], ring[i - 1]
        if (y1 > y) != (y2 > y) and x < (x2 - x1) * (y - y1) / (y2 - y1) + x1:
            inside = not inside
    return inside

def in_multipolygon(pt, geom):
    if not geom:
        return False
    polys = geom["coordinates"] if geom["type"] == "MultiPolygon" else [geom["coordinates"]]
    return any(in_ring(pt, poly[0]) and not any(in_ring(pt, h) for h in poly[1:]) for poly in polys)

# ---------------------------------------------------------------- inputs
pages = load(ROOT, "context_dev", "pages.json")
searches = load(ROOT, "context_dev", "search_results.json")
places = {k: v for k, v in load(ROOT, "context_dev", "places.json").items() if v}
dataset = load(REPO, "data", "historical", "arganil-piodao-2025", "dataset.json")
replay = load(REPO, "web", "public", "historical", "arganil-piodao-2025.json")
alias = {"Foz d’Égua": "Foz de Égua", "Chãs d’Égua": "Chãs de Égua", "Chãs d'Égua": "Chãs de Égua", "Chãs d’ Égua": "Chãs de Égua",
         "Coja (Arganil)": "Côja", "Coja": "Côja", "Vide (Seia, Guarda)": "Vide", "Aldeia das Dez (Oliveira do Hospital)": "Aldeia das Dez",
         "freguesia do Piódão": "Piódão", "Aldeia da Gramaça": "Gramaça", "Pampilhosa da Serra (Coimbra)": "Pampilhosa da Serra",
         "Casa do Povo de Vide": "Vide", "Escola Básica Integrada da Ponte das Três Entradas": "Ponte das Três Entradas"}

fogos_page = next(p for p in pages if p["url"] == "https://fogos.pt/pt/fogo/20251075979")
fogos = fogos_page["fogos"]
ignition = fogos["position"]

def fogos_time(s):
    d = datetime.strptime(s, "%d-%m-%Y %H:%M").replace(tzinfo=UTC)
    return d - LISBON_SUMMER

# ---------------------------------------------------------------- evidence
evidence, page_index = [], []
lineage_by_headline = {}
for i, page in enumerate(pages):
    pid = f"page-{i:02d}"
    if "fogos.pt" in page["url"]:
        times = {"published_at": None, "modified_at": None, "time_basis": "Fogos.pt status table (each status at its own time)",
                 "known_at": iso(fogos_time(fogos["status_history_local"][-1]["time_local"])), "time_flags": []}
    elif page["markdown_success"]:
        times = resolve_times(page)
    else:
        times = {"published_at": None, "modified_at": None, "time_basis": None, "known_at": None, "time_flags": ["capture failed"]}
    head = re.sub(r"\W+", " ", (page["headline"] or "").lower()).strip()
    lineage = lineage_by_headline.setdefault(head, f"lineage-{len(lineage_by_headline):02d}") if head else None
    page_index.append({"id": pid, "url": page["url"], "publisher": page["publisher"], "headline": page["headline"],
                       "request_id": page["request_id"], "captured_at": page["captured_at"], "credits": page["credits"],
                       "status": "ok" if page["markdown_success"] else "failed", "error_code": page["error_code"],
                       "mentions_lusa": page["mentions_lusa"], "lineage": lineage, **times})
    if "fogos.pt" in page["url"] or not page["markdown_success"]:
        continue
    for j, f in enumerate(page["facts"]):
        category, checks = f["category"], []
        if category == "event_gathering" and not EVENT_WORDS.search(f["quote"].lower()):
            # The extractor's category is not trusted on its own: an event must be named in the quote.
            category, checks = "people_present", ["category event_gathering -> people_present: no named event in quote"]
        event = None
        if category == "event_gathering":
            published = datetime.fromisoformat(times["known_at"].replace("Z", "+00:00")) + LISBON_SUMMER
            wd = next((k for k in WEEKDAYS if k in f["quote"].lower()), None)
            start_date = None
            if wd:
                ahead = (WEEKDAYS[wd] - published.weekday()) % 7 or 7
                start_date = (published + timedelta(days=ahead)).strftime("%Y-%m-%d")
            name = re.search(r"((?:Festa|Feira|Romaria|Festival|Arraial)(?:\s+(?:de|do|da|dos|das)\s+[A-ZÁÉÍÓÚÂÊÔÃÕÇ][\wáéíóúâêôãõç]+){1,2})", f["quote"])
            event = {"name": name.group(1) if name else None, "scheduled_start_text": wd,
                     "scheduled_start_date": start_date,
                     "scheduled_start_basis": f"weekday '{wd}' resolved to the next one after publication ({published.strftime('%a %d %b %Y')})" if wd else None,
                     "status": "cancelled" if "cancelad" in f["quote"].lower() else None}
        resolved = []
        for name in f["places"]:
            key = alias.get(name, name)
            g = places.get(key)
            resolved.append({"name": name, "geonames": key if g else None, "position": g["position"] if g else None})
        evidence.append({
            "id": f"{pid}-f{j:02d}", "page": pid, "category": category, "extracted_category": f["category"],
            "checks": checks, "event": event, "statement_en": f["statement_en"],
            "quote": f["quote"], "places": resolved, "time_text": f["time_text"], "number": f["number"], "unit": f["unit"],
            "quality": "verified" if f["quote_verified"] else "unverified",
            "number_check": None if f["number_in_quote"] is None else ("in_quote" if f["number_in_quote"] else "not_literal_in_quote"),
            "known_at": times["known_at"], "url": page["url"], "publisher": page["publisher"], "lineage": lineage,
            "mode": "historical",
        })

# Fogos.pt official status timeline (ANEPC data shown on a public page).
timeline = [{"at": iso(fogos_time(s["time_local"])), "status": s["status"], "source": "Fogos.pt public page (via Context.dev)",
             "url": fogos_page["url"], "request_id": fogos_page["request_id"]} for s in reversed(fogos["status_history_local"])]

# ---------------------------------------------------------------- weather (ERA5)
points = dataset["weather"]["points"]
nearest = min(points, key=lambda p: km(p["lon_lat"], ignition))
weather_hours = {h["time"]: h for h in nearest["hours"]}

def weather_at(t):
    key = t.replace(minute=0, second=0).strftime("%Y-%m-%dT%H:00:00Z")
    h = weather_hours.get(key)
    return None if h is None else {**h, "point": nearest["lon_lat"], "distance_km": round(km(nearest["lon_lat"], ignition), 1)}

# ---------------------------------------------------------------- frames
start, end = (datetime.fromisoformat(x.replace("Z", "+00:00")) for x in WINDOW)
frames = [f for f in replay["frames"] if start <= datetime.fromisoformat(f["t"].replace("Z", "+00:00")) <= end]
EXPOSURE_KIND = {"evacuation": "evacuated", "threatened_place": "threatened", "shelter": "shelter",
                 "people_present": "people_present", "event_gathering": "event", "road_closure": "road_closure"}
PEOPLE_UNITS = ("pessoa", "morador", "habitante", "residente", "aldeia")
os.makedirs(os.path.join(OUT, "frames"), exist_ok=True)
index_frames = []
for n, fr in enumerate(frames):
    t = datetime.fromisoformat(fr["t"].replace("Z", "+00:00"))
    known = [e for e in evidence if e["known_at"] and e["known_at"] <= fr["t"]]
    status = [s for s in timeline if s["at"] <= fr["t"]]
    # Exposures: places named in known facts, with what was said about them.
    exp = {}
    for e in known:
        kind = EXPOSURE_KIND.get(e["category"])
        if not kind:
            continue
        for p in e["places"]:
            if not p["geonames"] or p["geonames"] in ("Arganil", "Seia", "Oliveira do Hospital", "Pampilhosa da Serra"):
                continue  # municipalities are context, not exposure sites
            g = places[p["geonames"]]
            x = exp.setdefault(p["geonames"], {"id": f"geonames-{g['geonames_id']}", "name": g["name"], "kind": set(),
                "position": g["position"], "people": None, "people_basis": None, "population_geonames": g["population"],
                "distance_from_ignition_km": round(km(g["position"], ignition), 1), "events": [], "evidence": []})
            x["kind"].add(kind)
            x["evidence"].append(e["id"])
            if e["event"] and e["event"] not in x["events"]:
                x["events"].append({**e["event"], "evidence": e["id"]})
            if e["number"] and e["unit"] and any(u in e["unit"].lower() for u in PEOPLE_UNITS[:4]) and len(e["places"]) == 1:
                x["people"], x["people_basis"] = e["number"], f"stated: {e['id']}"
    exposures = []
    for x in exp.values():
        pos = x["position"]
        exposures.append({**x, "kind": sorted(x["kind"]),
            "in_modelled_burn_p50": in_multipolygon(pos, fr["predicted"].get("p50")),
            "in_forecast_6h": in_multipolygon(pos, fr["forecast"].get("h6")),
            "in_forecast_12h": in_multipolygon(pos, fr["forecast"].get("h12"))})
    exposures.sort(key=lambda x: x["distance_from_ignition_km"])
    automation = []
    names = {"Início": "reported", "Despacho": "dispatched", "Despacho de 1º Alerta": "first_alert_dispatch", "Em Curso": "active"}
    t0 = timeline[0]["at"]
    for s in status:
        if s["status"] in names:
            automation.append({"step": names[s["status"]], "at": s["at"], "basis": "observed", "detail": f"Fogos.pt status: {s['status']}", "url": s["url"]})
    first_news = min((e for e in known if e["category"] in ("ignition_cause", "fire_status", "evacuation")), key=lambda e: e["known_at"], default=None)
    if first_news:
        automation.append({"step": "corroborated_by_news", "at": first_news["known_at"], "basis": "observed", "detail": first_news["publisher"], "url": first_news["url"]})
    if fr["t"] >= t0:
        automation.append({"step": "forecast", "at": t0, "basis": "simulated", "detail": f"EmberWatch firecast run (measured {replay['meta'].get('runtime_ms')} ms for the replay)", "url": None})
        automation.append({"step": "drones_launched", "at": t0, "basis": "simulated", "detail": f"{fr['drones'].get('count', 0)} simulated drones in this frame", "url": None})
    for a in automation:
        a["latency_s"] = int((datetime.fromisoformat(a["at"].replace("Z", "+00:00")) - datetime.fromisoformat(t0.replace("Z", "+00:00"))).total_seconds()) if a["at"] >= t0 else None
    frame = {
        "t": fr["t"], "mode": "historical",
        "weather": weather_at(t),
        "model": fr["model"], "forecast": fr["forecast"], "predicted": fr["predicted"], "smoke": fr["smoke"],
        "smoke_axis": fr["smoke_axis"], "plume": fr["plume"], "wind": fr["wind"],
        "drones_legacy": fr["drones"], "fleet": None, "sitreps": [], "fleet_check": None,
        "fogos_status": status[-1] if status else None,
        "evidence_ids": [e["id"] for e in known],
        "exposures": exposures,
        "automation": sorted(automation, key=lambda a: a["at"]),
    }
    name = f"frames/{n:02d}.json"
    json.dump(frame, open(os.path.join(OUT, name), "w", encoding="utf-8"), ensure_ascii=False, separators=(",", ":"))
    index_frames.append({"t": fr["t"], "file": name, "evidence": len(known), "exposures": len(exposures),
                         "area_ha_p50": fr["model"].get("area_ha_p50")})

monitors = [
    {"id": "fogos", "category": "official_incident_page", "provider": "context.dev web/scrape", "target": "https://fogos.pt/pt/fogo/20251075979",
     "status": "ok", "items": len(timeline), "note": "Status table and official coordinates. Present-day meteo and resource counters on the page are excluded."},
    {"id": "news_index", "category": "news", "provider": "context.dev news/search", "target": "natural-language query, 13–14 Aug 2025 window",
     "status": "coverage_gap", "items": 0, "note": "Portuguese-language results were off-topic; 'pt' is not an accepted sourceCountry and Portuguese publisher domains returned no results. English international coverage only mentions Portugal in passing."},
    {"id": "web_news", "category": "news", "provider": "context.dev web/search + web/scrape", "target": "Portuguese queries: fire, evacuations, roads",
     "status": "ok", "items": sum(1 for p in page_index if p["status"] == "ok" and "fogos" not in p["url"]),
     "note": "Regional and national outlets; facts extracted with a JSON schema and checked against page text."},
    {"id": "events", "category": "events_near_fire", "provider": "context.dev web/search + web/scrape", "target": "festas agosto 2025 Arganil Piódão",
     "status": "ok", "items": sum(1 for e in evidence if e["category"] == "event_gathering"), "note": "Festa de Verão do Piódão found; social-media announcements were not scraped."},
    {"id": "warnings", "category": "fire_danger_and_weather_warnings", "provider": "context.dev web/search + web/scrape", "target": "IPMA warnings reported 11–12 Aug 2025",
     "status": "ok", "items": sum(1 for e in evidence if e["category"] == "fire_danger_warning"), "note": "IPMA's API only serves current warnings, so historical warnings come from dated news reports of IPMA notices."},
    {"id": "weather", "category": "weather", "provider": "Open-Meteo archive (ERA5 reanalysis)", "target": "nearest of 9 grid points",
     "status": "ok", "items": len(weather_hours), "note": "Temperature, humidity, 10 m and 100 m wind speed/direction, gusts, cloud, radiation, rain, VPD."},
    {"id": "satellite", "category": "satellite_hotspots", "provider": "NASA FIRMS", "target": "VIIRS/MODIS hotspots", "status": "not_run", "items": 0,
     "note": "Needs FIRMS_MAP_KEY; not available in this session."},
    {"id": "terrain", "category": "terrain_fuel", "provider": "Terrarium DEM (AWS Open Data); vegetation assumption", "target": "42 km domain, 200 m cells",
     "status": "ok", "items": 1, "note": "OSM land cover was unavailable; altitudinal vegetation assumption documented in the dataset."},
]

index = {
    "dataset": "arganil-early", "mode": "historical",
    "title": "Arganil / Piódão wildfire — first 42 hours (13–14 Aug 2025)",
    "window": {"from": WINDOW[0], "to": WINDOW[1]},
    "ignition": {"position": ignition, "source": "Fogos.pt public page (ANEPC data) via Context.dev", "url": fogos_page["url"],
                 "start_local": fogos["start_text"], "nature": fogos["nature"], "place": fogos["place_text"],
                 "model_ignition": dataset["ignition_position"], "model_ignition_note": "Model frames were run from Piódão village coordinates, about 0.4 km from the Fogos.pt point."},
    "timeline": timeline,
    "frames": index_frames,
    "pages": page_index,
    "evidence": evidence,
    "searches": [{k: s[k] for k in ("name", "endpoint", "request_id", "credits", "error")} | {"results": len(s["results"])} for s in searches],
    "monitors": monitors,
    "places_source": "GeoNames via Open-Meteo geocoding API (https://geocoding-api.open-meteo.com); nearest match within 30 km of the ignition",
    "weather_source": dataset["weather_source"],
    "model": replay["meta"].get("model"),
    "limitations": [
        "Pages were captured through Context.dev on " + min(p["captured_at"] for p in page_index if p["captured_at"]) + " (retrospectively). Each item is revealed at the earliest time it was provably public; live-updated pages are revealed at the latest clock time they mention.",
        "Captured text is today's version of each page; edits after publication cannot be ruled out.",
        "Drones and dispatch are simulations. 'drones_legacy' uses the old heuristic until the fleet model replaces it.",
        "Model frames are 3-hourly and run from Piódão village coordinates (0.4 km from the official point).",
        "Two pages were blocked by the website (WEBSITE_BLOCKED) and contribute nothing.",
    ],
}
json.dump(index, open(os.path.join(OUT, "index.json"), "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print(f"{len(index_frames)} frames, {len(evidence)} facts ({sum(e['quality']=='verified' for e in evidence)} verified), "
      f"{len(timeline)} Fogos statuses -> {os.path.relpath(OUT, REPO)}")
for p in page_index:
    print(f"  {p['known_at'] or '—':20} {p['publisher'][:26]:26} {'; '.join(p['time_flags'])[:110]}")
