"""Scrape selected public pages through Context.dev (markdown + schema JSON).
Raw responses stay in the scratchpad (copyrighted page text is not committed).
Usage: python3 -I scrape.py urls.json outdir max_credits
"""
import hashlib, json, os, sys, time, urllib.request

urls = json.load(open(sys.argv[1]))
out = sys.argv[2]
ceiling = float(sys.argv[3])
os.makedirs(out, exist_ok=True)

S = lambda d: {"type": ["string", "null"], "description": d}
SCHEMA = {
    "type": "object",
    "additionalProperties": False,
    "required": ["published_text", "updated_text", "headline", "facts"],
    "properties": {
        "published_text": S("Publication date/time exactly as printed on the page, verbatim; null if not shown."),
        "updated_text": S("Last-updated date/time exactly as printed, verbatim; null if not shown."),
        "headline": S("Main headline, verbatim."),
        "facts": {
            "type": "array",
            "maxItems": 20,
            "items": {
                "type": "object",
                "additionalProperties": False,
                "required": ["category", "statement_en", "places", "time_text", "number", "unit", "quote"],
                "properties": {
                    "category": {"type": "string", "enum": [
                        "ignition_cause", "ignition_time", "fire_status", "spread_direction",
                        "evacuation", "threatened_place", "road_closure", "resources",
                        "weather", "fire_danger_warning", "event_gathering", "people_present",
                        "shelter", "infrastructure", "other"]},
                    "statement_en": {"type": "string", "description": "One short English sentence stating only what the quote says."},
                    "places": {"type": "array", "items": {"type": "string"}, "maxItems": 12,
                               "description": "Place names exactly as written in the quote."},
                    "time_text": S("Time or date words from the quote, verbatim; null if none."),
                    "number": {"type": ["number", "null"], "description": "The single main number in the quote (people, operacionais, vehicles, aircraft, km/h, °C), else null."},
                    "unit": S("What the number counts, e.g. 'operacionais', 'meios aéreos', 'aldeias', 'km/h', '°C'; null if no number."),
                    "quote": {"type": "string", "description": "Verbatim sentence from the page (Portuguese), at most 300 characters, copied exactly."},
                },
            },
        },
    },
}
INSTRUCTIONS = ("Extract only facts printed on this page about the wildfire, its surroundings and the people, "
    "events, roads and weather near it. Copy quotes and dates verbatim. Never infer, translate or compute "
    "values that are not printed. Missing values are null. Ignore navigation, ads and other articles.")

used = 0.0
for url in urls:
    name = hashlib.sha1(url.encode()).hexdigest()[:12]
    path = f"{out}/scrape_{name}.json"
    if os.path.exists(path):
        print("cached", url); continue
    if used + 5 > ceiling:
        print("credit ceiling reached; stopping"); break
    body = {"url": url, "formats": {"markdown": True, "json": True},
            "timeoutOpts": {"milliseconds": 30000, "behavior": "return-partial"},
            "jsonParams": {"schema": SCHEMA, "instructions": INSTRUCTIONS}}
    req = urllib.request.Request("https://api.context.dev/v1/web/scrape", data=json.dumps(body).encode(),
        headers={"Authorization": "Bearer proxy-injected", "Content-Type": "application/json"})
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=90) as r:
            d = json.load(r)
            status = r.status
    except urllib.error.HTTPError as e:
        d = {"http_error": e.code, "body": e.read().decode(errors="replace")[:500]}; status = e.code
    except Exception as e:  # network/timeouts: outcome and credits unknown
        d = {"error": str(e)}; status = None
    d["_captured_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    d["_requested_url"] = url
    d["_latency_s"] = round(time.time() - t0, 1)
    c = (d.get("key_metadata") or {}).get("credits_consumed") or 0
    used += c
    json.dump(d, open(path, "w"), ensure_ascii=False)
    md = (d.get("markdown") or {})
    js = (d.get("json") or {})
    print(status, c, "md", md.get("success"), "json", js.get("success"), "partial", d.get("isPartial"), url[:90])
print("credits used", used)
