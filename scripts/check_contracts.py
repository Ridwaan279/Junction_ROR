"""Validate OpenAPI references, JSON Schema, and the synthetic integration fixture."""
import json
from pathlib import Path

import yaml
from jsonschema import validators, FormatChecker
from openapi_spec_validator import OpenAPIV31SpecValidator

root = Path(__file__).resolve().parents[1]
spec_path = root / "contracts" / "openapi.yaml"
spec = yaml.safe_load(spec_path.read_text(encoding="utf-8"))
errors = list(OpenAPIV31SpecValidator(spec, base_uri=spec_path.as_uri()).iter_errors())
if errors:
    for error in errors:
        print(f"{'/'.join(map(str, error.absolute_path))}: {error.message}")
    raise SystemExit(1)
schema = json.loads((root / "contracts" / "schema.json").read_text(encoding="utf-8"))
Validator = validators.validator_for(schema)
Validator.check_schema(schema)
defs_key = "$defs" if "$defs" in schema else "definitions"
definitions = schema[defs_key]
dataset_schema = {
    "$schema": schema["$schema"],
    "$ref": f"#/{defs_key}/ScenarioDataset",
    defs_key: definitions,
}
fixture = json.loads(
    (root / "contracts" / "fixtures" / "portugal-synthetic.json").read_text(encoding="utf-8")
)
Validator(dataset_schema, format_checker=FormatChecker()).validate(fixture)
response = json.loads(
    (root / "contracts" / "fixtures" / "response-synthetic.json").read_text(encoding="utf-8")
)
for field, definition in [("forecast", "Forecast"), ("drone_plan", "DronePlan"), ("briefing", "Briefing")]:
    shape = {"$ref": f"#/{defs_key}/{definition}", defs_key: definitions}
    Validator(shape, format_checker=FormatChecker()).validate(response[field])
source_shape = {"$ref": f"#/{defs_key}/SourceDescriptor", defs_key: definitions}
sources = json.loads(
    (root / "contracts" / "fixtures" / "source-health.json").read_text(encoding="utf-8")
)
for source in sources:
    Validator(source_shape, format_checker=FormatChecker()).validate(source)
print("OpenAPI 3.1, shared JSON Schema, and Portugal fixture validated.")
