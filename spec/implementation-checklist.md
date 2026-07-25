# Initial Worker Vertical Slice Checklist

This checklist tracks the first production slice defined by the root `architecture.md` and the implementation request.

## Repository baseline

- [x] Inspect repository metadata, default branch, history, branches, issues and pull requests.
- [x] Read the root architecture document and preserve it as the canonical design authority.
- [x] Confirm no existing Rust workspace, implementation, tests or CI are present to reconcile.
- [x] Create the dedicated branch `feat/worker-foundation`.

## Workspace and contracts

- [ ] Create the seven-crate Rust workspace described by the architecture.
- [ ] Define API request, response and RFC 9457 problem models.
- [ ] Define stable error codes and OpenAPI 3.0 schema.
- [ ] Define engine descriptors, traits, registry, state, transport and telemetry boundaries.

## Core behavior

- [ ] Implement query normalization and validation.
- [ ] Implement URL canonicalization and exact-URL deduplication.
- [ ] Implement `rrf-v1` and `searx-compat-v1` ranking.
- [ ] Implement deterministic cache keys and cursor signing interface.
- [ ] Implement concurrent execution with partial-result semantics.

## Worker runtime

- [ ] Add the `workers-rs` entrypoint and Router routes.
- [ ] Add bearer authentication with constant-time comparison.
- [ ] Add request IDs, structured responses and structured tracing.
- [ ] Add Cache API integration and optional KV/Analytics Engine bindings.
- [ ] Add restricted Workers Fetch transport with host, redirect, deadline, body and content-type limits.

## Engines and parsers

- [ ] Implement reusable bounded HTML extraction with `lol-html`.
- [ ] Implement Atom/XML parsing with `quick-xml`.
- [ ] Implement arXiv.
- [ ] Implement Wikipedia.
- [ ] Implement DuckDuckGo HTML first-page search.
- [ ] Add engine manifests and provenance records.

## Verification and operations

- [ ] Add unit, fixture, contract and Worker integration tests.
- [ ] Add fixture sets for normal, empty, changed, denied, limited, challenged, truncated, oversized, wrong-content-type and redirect responses.
- [ ] Add CI for fmt, Clippy, tests, Wasm build, schemas, Wrangler/workerd integration and bundle size.
- [ ] Add local development, deployment, security, licensing, provider-disable and engine-authoring documentation.
- [ ] Run all available verification commands and record exact results in the pull request.
