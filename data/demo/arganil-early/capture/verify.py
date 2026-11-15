"""Quote verification for Context.dev scrape captures: every extracted quote must be
a substring of the scraped Markdown after normalisation. Writes verified.json."""
import glob, json, re, sys, unicodedata

def norm(s):
    s = unicodedata.normalize("NFC", s or "")
    s = re.sub(r"!\[[^\]]*\]\([^)]*\)", " ", s)          # images
    s = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", s)        # links -> text
    s = re.sub(r"[*_`#>]+", "", s)
    s = s.replace(" ", " ").replace("“", '"').replace("”", '"').replace("’", "'").replace("‘", "'")
    s = s.replace("–", "-").replace("—", "-")
    return re.sub(r"\s+", " ", s).strip().lower()

out = []
for f in sorted(glob.glob(sys.argv[1] + "/scrape_*.json")):
    d = json.load(open(f))
    md = (d.get("markdown") or {}).get("data") or ""
    js = (d.get("json") or {}).get("data") or {}
    page = norm(md)
    facts = []
    for fact in js.get("facts") or []:
        q = norm(fact.get("quote"))
        ok = bool(q) and q in page
        num_ok = None
        if fact.get("number") is not None:
            n = fact["number"]
            reps = {f"{n:g}", f"{n:,.0f}".replace(",", "."), f"{n:,.0f}".replace(",", " "), str(int(n)) if float(n).is_integer() else f"{n}".replace(".", ",")}
            num_ok = any(r in fact.get("quote", "") for r in reps)
        facts.append({**fact, "quote_verified": ok, "number_in_quote": num_ok})
    pub = js.get("published_text")
    out.append({
        "url": d.get("_requested_url"),
        "final_url": (d.get("metadata") or {}).get("finalUrl"),
        "page_title": (d.get("metadata") or {}).get("title"),
        "captured_at": d.get("_captured_at"),
        "request_id": d.get("request_id"),
        "credits": (d.get("key_metadata") or {}).get("credits_consumed"),
        "markdown_success": (d.get("markdown") or {}).get("success"),
        "json_success": (d.get("json") or {}).get("success"),
        "error_code": (d.get("markdown") or {}).get("error_code") or (d.get("json") or {}).get("error_code"),
        "is_partial": d.get("isPartial"),
        "headline": js.get("headline"),
        "published_text": pub,
        "published_in_page": bool(pub) and norm(pub) in page,
        "updated_text": js.get("updated_text"),
        "facts": facts,
    })
json.dump(out, open(sys.argv[2], "w"), ensure_ascii=False, indent=1)
tot = sum(len(p["facts"]) for p in out)
ver = sum(f["quote_verified"] for p in out for f in p["facts"])
nums = [f["number_in_quote"] for p in out for f in p["facts"] if f["number_in_quote"] is not None]
print(f"pages {len(out)}  facts {tot}  quotes verified {ver}/{tot}  numbers in quote {sum(nums)}/{len(nums)}")
for p in out:
    v = sum(f["quote_verified"] for f in p["facts"])
    print(f"{v:>2}/{len(p['facts']):<2} pub={p['published_text']!r:32.32} inpage={p['published_in_page']!s:5} err={p['error_code']} {p['url'][8:70]}")
