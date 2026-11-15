# Arganil golden candidates — not an approved benchmark

The manifest references the 25 real captured pages supplied with the demo,
including two failed captures. The user's original ZIP has now been preserved
privately; 23 original Markdown hashes were added to `pages.json` after matching
request IDs. The export itself retains short facts and prior verification flags,
not full article text. Those flags are not independent gold labels.

Set `EMBERWATCH_GOLDEN_RAW_DIR` to the private `verified-originals` directory and
run `cargo test -p emberwatch-intelligence golden -- --nocapture` to reproduce
exact-byte hashes and candidate quote/number support against original responses.
These are source-grounding checks, not a human-labelled recall benchmark.

For each page a person must read the private original capture, write expected
fields in `expected.json`, and fill reviewer/source-text references. Until then,
all independent benchmark percentages are **not assessed**, and Gate 3 is **not passed**.
The current `golden` test verifies the provenance inventory and this pending state;
it is not the threshold evaluator and does not imply that the thresholds pass.

Do not lower the thresholds, label candidates as human-reviewed, invent raw
captures, or count successful inventory checks as a 95% extraction result.
Keep publisher text out of Git pending rights review; references/short retained
facts can be used with the existing supplied dataset.
