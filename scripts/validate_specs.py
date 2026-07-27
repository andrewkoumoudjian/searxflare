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

    with (ROOT / "spec" / "provider-policies.yaml").open(encoding="utf-8") as handle:
        policy_registry = yaml.safe_load(handle)
    providers = policy_registry.get("providers", {})
    manifest_ids = {
        yaml.safe_load(path.read_text(encoding="utf-8"))["id"] for path in manifests
    }
    if set(providers) != manifest_ids:
        missing = sorted(manifest_ids - set(providers))
        extra = sorted(set(providers) - manifest_ids)
        raise SystemExit(
            f"provider policy registry mismatch; missing={missing}, extra={extra}"
        )
    allowed_contracts = {
        "public_api",
        "public_frontend",
        "supported_api",
        "supported_mcp",
    }
    for provider_id, policy in providers.items():
        if policy.get("contract") not in allowed_contracts:
            raise SystemExit(f"invalid provider contract for {provider_id}")
        if not isinstance(policy.get("production_default"), bool):
            raise SystemExit(f"production_default must be boolean for {provider_id}")

    for schema_name in ["result.schema.json", "error.schema.json"]:
        Draft202012Validator.check_schema(load_json(ROOT / "spec" / schema_name))

    print(
        f"validated OpenAPI, {len(manifests)} engine manifests, "
        "provider policies, and JSON schemas"
    )


if __name__ == "__main__":
    main()
