# Arganil / Piódão early-stage demo inputs

Real, public information about the first 42 hours of the 2025 Arganil/Piódão fire,
captured through Context.dev. See [docs/DEMO_DATA.md](../../../docs/DEMO_DATA.md)
for sources, accuracy rules and the output format.

- `context_dev/`: extracted facts with short verbatim quotes, request IDs, credits
  and failures. Full page text is **not** committed: public visibility does not grant
  reuse rights.
- `capture/`: the exact request bodies (`queries.json`) and the scripts that ran
  them. Re-capturing needs a Context.dev key on the server and spends credits:

  ```bash
  python3 -I capture/scrape.py <urls.json> <raw-dir> <max-credits>
  python3 -I capture/verify.py <raw-dir> verified.json
  python3 -I capture/export.py <raw-dir> verified.json places.json context_dev
  ```

- `build.py`: builds `web/public/fixtures/v2/arganil-early/` offline.
