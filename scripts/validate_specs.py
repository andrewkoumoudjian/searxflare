#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path

import yaml
from jsonschema import Draft202012Validator, FormatChecker
from openapi_spec_validator import validate_spec

ROOT = Path(__file__).resolve().parents[1]


def load_json(path: Path):
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def main() -> None:
    openapi_path = ROOT / "spec" / "openapi.yaml"
    with openapi_path.open(encoding="utf-8") as handle:
        openapi = yaml.safe_load(handle)
    validate_spec(openapi)

    manifest_schema = load_json(ROOT / "spec" / "engine-manifest.schema.json")
    validator = Draft202012Validator(manifest_schema, format_checker=FormatChecker())
    manifests = sorted((ROOT / "spec" / "engines").glob("*.yaml"))
    if not manifests:
        raise SystemExit("no engine manifests found")
    for path in manifests:
        with path.open(encoding="utf-8") as handle:
            manifest = yaml.safe_load(handle)
        errors = sorted(validator.iter_errors(manifest), key=lambda error: list(error.path))
        if errors:
            rendered = "\n".join(f"{path}: {error.message}" for error in errors)
            raise SystemExit(rendered)

    for schema_name in ["result.schema.json", "error.schema.json"]:
        Draft202012Validator.check_schema(load_json(ROOT / "spec" / schema_name))

    print(f"validated OpenAPI, {len(manifests)} engine manifests, and JSON schemas")


if __name__ == "__main__":
    main()
