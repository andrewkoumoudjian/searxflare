# Milestone 5: General Web Provider Diversity

This checklist tracks Milestone 5 of the canonical `architecture.md`.

## Provider implementation

- [x] Add a fixed-host Brave Web HTML adapter.
- [x] Add a fixed-host Qwant Web JSON adapter.
- [x] Preserve DuckDuckGo HTML as an independent general-web provider.
- [x] Keep all adapters compatible with `wasm32-unknown-unknown` and Workers Fetch.
- [x] Bound paging, redirects, response bodies, steps and provider deadlines.
- [x] Support locale, country and safe-search where the provider surface permits it.
- [x] Classify provider rate limits, challenges, access denial, invalid content and parse failures without failing successful engines.

## Parsing and fixtures

- [x] Add normal, empty and changed-layout Brave fixtures.
- [x] Add normal, empty, changed-schema and rate-limit Qwant fixtures.
- [x] Add unit tests for request construction, paging limits, capability rejection and parsing.
- [x] Extend workerd integration coverage to the five-engine catalogue and three-provider general-web fan-out.

Transport-level denied, challenged, oversized, wrong-content-type and redirect behavior remains covered by the shared HTTP transport fixture suite rather than duplicated inside every engine directory.

## Documentation and provenance

- [x] Add engine manifests validated by the existing manifest schema.
- [x] Record the exact SearXNG reference commit and files in `SOURCE-NOTES.md`.
- [x] Document provider-controlled contract and state limitations.
- [x] Add local request examples for the independent general-web provider set.

## Validation evidence

- [x] Cloudflare Git preview compiled and uploaded the exact implementation head as a Worker version.
- [ ] Formatting, Clippy, native workspace tests, Wasm workspace build, schemas, workerd integration tests, Wrangler dry-run and bundle-size checks pass on the final head.
- [ ] The final branch preview responds correctly to health, readiness, authentication and search requests.
- [ ] Production account authentication is configured and live provider egress is verified after merge/deployment.

The last production item requires the account-level `API_KEY_SHA256` secret and is intentionally not satisfied by repository code or a preview build alone.
