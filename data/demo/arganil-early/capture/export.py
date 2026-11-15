"""Export Context.dev captures to committed provenance files (no full page text).
python3 -I export.py ctxraw verified.json geo/places.json OUTDIR"""
import glob, json, os, re, sys
from urllib.parse import urlparse

raw_dir, verified_path, places_path, out = sys.argv[1:5]
os.makedirs(out, exist_ok=True)
verified = {p["url"]: p for p in json.load(open(verified_path))}

def clip(s, n=300):
    s = re.sub(r"\s+", " ", s or "").strip()
    return s if len(s) <= n else s[: n - 1] + "…"

pages, fogos = [], None
for f in sorted(glob.glob(raw_dir + "/scrape_*.json")):
    d = json.load(open(f))
    url = d["_requested_url"]
    v = verified[url]
    md = (d.get("markdown") or {}).get("data") or ""
    meta = d.get("metadata") or {}
    page = {
        "url": url,
        "final_url": meta.get("finalUrl") or v.get("final_url"),
        "publisher": urlparse(url).netloc.removeprefix("www."),
        "page_title": meta.get("title"),
        "headline": v.get("headline"),
        "endpoint": "POST /v1/web/scrape (formats: markdown + json schema)",
        "request_id": d.get("request_id"),
        "captured_at": d.get("_captured_at"),
        "credits": (d.get("key_metadata") or {}).get("credits_consumed"),
        "markdown_success": v["markdown_success"],
        "json_success": v["json_success"],
        "error_code": v["error_code"],
        "is_partial": d.get("isPartial"),
        "meta_published": meta.get("publishedTime"),
        "meta_modified": meta.get("modifiedTime"),
        "printed_published": v["published_text"],
        "printed_published_in_page": v["published_in_page"],
        "printed_updated": v["updated_text"],
        "mentions_lusa": bool(re.search(r"\bLusa\b", md)),
        "markdown_chars": len(md),
        "facts": [
            {
                "category": x["category"],
                "statement_en": x["statement_en"],
                "places": x["places"],
                "time_text": x["time_text"],
                "number": x["number"],
                "unit": x["unit"],
                "quote": clip(x["quote"]),
                "quote_verified": x["quote_verified"],
                "number_in_quote": x["number_in_quote"],
            }
            for x in v["facts"]
        ],
    }
    if "fogos.pt" in url and md:
        # Parse the public status table exactly as rendered ("dd-mm-yyyy HH:MM" then status).
        block = md.split("#### Estado", 1)[1].split("####", 1)[0] if "#### Estado" in md else ""
        statuses = re.findall(r"(\d{2}-\d{2}-\d{4} \d{2}:\d{2})\s*\n+\s*([^\n]+)", block)
        loc = re.search(r"###### Localização\s+([-\d.]+),\s*([-\d.]+)", md)
        start = re.search(r"###### Início\s+(\d{2}:\d{2} \d{2}-\d{2}-\d{4})", md)
        nature = re.search(r"###### Natureza\s+([^\n]+)", md)
        place = re.search(r"###### Local\s+([^\n\[]+)", md)
        weather_at = re.search(r"Dados de: ([^\n]+)", md)
        page["fogos"] = {
            "occurrence_id": "20251075979",
            "place_text": place.group(1).strip() if place else None,
            "start_text": start.group(1) if start else None,
            "nature": nature.group(1).strip() if nature else None,
            "position": [float(loc.group(2)), float(loc.group(1))] if loc else None,
            "status_history_local": [{"time_local": t, "status": s.strip()} for t, s in statuses],
            "excluded": [
                "Meteo block: current station reading dated " + (weather_at.group(1).strip() if weather_at else "unknown") + ", not the fire period.",
                "Meios '0 0 0': present-day value after the incident closed, not the historical deployment.",
            ],
        }
        fogos = fogos or page["fogos"]
    pages.append(page)

searches = []
for f in sorted(glob.glob(raw_dir + "/news_*.json") + glob.glob(raw_dir + "/web_*.json")):
    d = json.load(open(f))
    name = os.path.basename(f)[:-5]
    rows = d.get("data") if name.startswith("news_") else d.get("results")
    searches.append({
        "name": name,
        "endpoint": "POST /v1/news/search" if name.startswith("news_") else "POST /v1/web/search",
        "request_id": d.get("request_id"),
        "credits": (d.get("key_metadata") or {}).get("credits_consumed"),
        "error": d.get("error") or d.get("message"),
        "results": [
            {
                "url": r.get("url"),
                "title": r.get("title"),
                "description": clip(r.get("description"), 300),
                "published_at": r.get("published_at"),
                "relevance": r.get("relevance"),
                "source_domain": (r.get("source") or {}).get("domain"),
            }
            for r in rows or []
        ],
    })

json.dump(pages, open(out + "/pages.json", "w"), ensure_ascii=False, indent=1)
json.dump(searches, open(out + "/search_results.json", "w"), ensure_ascii=False, indent=1)
json.dump(json.load(open(places_path)), open(out + "/places.json", "w"), ensure_ascii=False, indent=1)
print(len(pages), "pages;", len(searches), "searches; fogos statuses:", fogos and len(fogos["status_history_local"]))
print(json.dumps(fogos, ensure_ascii=False, indent=1))
