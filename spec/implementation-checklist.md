# Initial Worker Vertical Slice Checklist

This checklist tracks the first production slice defined by the root `architecture.md` and the implementation request.

Validation completed on 2026-07-25 against runtime commit `84b917905f206744a9610c9a198383ceb9f5f330`. Subsequent commits synchronize documentation only.

## Repository baseline

- [x] Inspect repository metadata, default branch, history, branches, issues and pull requests.
- [x] Read the root architecture document and preserve it as the canonical design authority.
- [x] Confirm no existing Rust workspace, implementation, tests or CI are present to reconcile.
- [x] Create the dedicated branch `feat/worker-foundation`.

## Workspace and contracts

- [x] Create the seven-crate Rust workspace described by the architecture.
- [x] Define API request, response and RFC 9457 problem models.
- [x] Define stable error codes and OpenAPI 3.0 schema.
- [x] Define engine descriptors, traits, registry, state, transport and telemetry boundaries.

## Core behavior

- [x] Implement query normalization and validation.
- [x] Implement URL canonicalization and exact-URL deduplication.
- [x] Implement `rrf-v1` and `searx-compat-v1` ranking.
- [x] Implement deterministic cache keys and cursor signing interface.
- [x] Implement concurrent execution with partial-result semantics.

## Worker runtime

- [x] Add the `workers-rs` entrypoint and Router routes.
- [x] Add bearer authentication with constant-time comparison.
- [x] Add request IDs, structured responses and structured tracing.
- [x] Add Cache API integration and optional KV/Analytics Engine bindings.
- [x] Add restricted Workers Fetch transport with host, redirect, deadline, body and content-type limits.

## Engines and parsers

- [x] Implement reusable bounded HTML extraction with `lol-html`.
- [x] Implement Atom/XML parsing with `quick-xml`.
- [x] Implement arXiv.
- [x] Implement Wikipedia.
- [x] Implement DuckDuckGo HTML first-page search.
- [x] Add engine manifests and provenance records.

## Verification and operations

- [x] Add unit, fixture, contract and Worker integration tests.
- [x] Add fixture sets for normal, empty, changed, denied, limited, challenged, truncated, oversized, wrong-content-type and redirect responses.
- [x] Add CI for fmt, Clippy, tests, Wasm build, schemas, Wrangler/workerd integration and bundle size.
- [x] Add local development, deployment, security, licensing, provider-disable and engine-authoring documentation.
- [x] Run all available verification commands and record exact results in the pull request.
