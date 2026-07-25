# Source Notes

Reviewed: 2026-07-25

## Cloudflare platform sources

- Workers Fetch API: https://developers.cloudflare.com/workers/runtime-apis/fetch/
- Workers request redirects: https://developers.cloudflare.com/workers/runtime-apis/request/
- Workers Git integration: https://developers.cloudflare.com/workers/ci-cd/builds/git-integration/
- Workers Builds: https://developers.cloudflare.com/workers/ci-cd/builds/
- Workers secrets: https://developers.cloudflare.com/workers/configuration/secrets/
- Workers versions and deployments: https://developers.cloudflare.com/workers/configuration/versions-and-deployments/

These sources were used to confirm the Worker Fetch execution model, preview deployment behavior, secret handling, and deployment/version inspection. Live account state was checked separately through Cloudflare Code Mode; documentation is not treated as proof of deployment.

## SearXNG behavioral references

Upstream repository: https://github.com/searxng/searxng

Reference commit: `0909dbc9efb2c6e93e2ad51e60e66417ab291710`

Files consulted:

- `searx/engines/brave.py`
- `searx/engines/qwant.py`

The reference was used to identify public frontend endpoints, stable query parameters, cookie names, result containers, paging limits, safe-search mapping, locale handling, and provider error shapes. Searxflare does not copy SearXNG control flow or Python implementation. The Rust adapters use the repository's own engine traits, restricted transport, bounded parsers, failure taxonomy, caching, ranking, and `wasm32-unknown-unknown` constraints.

SearXNG is AGPL-3.0-or-later. These notes preserve provenance and make the clean-room behavioral adaptation explicit. See `spec/licensing.md` for the repository's licensing policy.

## Provider surfaces

- Brave public search frontend: `https://search.brave.com/search`
- Qwant public frontend JSON surface: `https://api.qwant.com/v3/search/web`

Neither surface is a supported public API contract. Provider behavior can change, rate-limit, challenge, or deny Cloudflare egress. The implementation does not bypass those controls; failures remain isolated and observable.
