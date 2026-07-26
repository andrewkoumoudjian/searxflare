# Source Notes

Reviewed: 2026-07-26

## Cloudflare platform sources

- Workers Fetch API: https://developers.cloudflare.com/workers/runtime-apis/fetch/
- Workers request redirects: https://developers.cloudflare.com/workers/runtime-apis/request/
- Workers Git integration: https://developers.cloudflare.com/workers/ci-cd/builds/git-integration/
- Workers Builds: https://developers.cloudflare.com/workers/ci-cd/builds/
- Workers secrets: https://developers.cloudflare.com/workers/configuration/secrets/
- Workers versions and deployments: https://developers.cloudflare.com/workers/versions-and-deployments/
- Workers preview URLs: https://developers.cloudflare.com/workers/versions-and-deployments/preview-urls/

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

## Academic provider APIs

- NCBI E-utilities usage and ESearch/ESummary flow: https://www.ncbi.nlm.nih.gov/books/NBK25500/
- Semantic Scholar Academic Graph API: https://api.semanticscholar.org/api-docs/graph
- Crossref REST API: https://crossref.gitlab.io/rest-api-doc/
- OpenAlex API overview and authentication: https://developers.openalex.org/

PubMed uses a bounded ESearch JSON request followed by ESummary JSON only when IDs are returned. Semantic Scholar uses relevance-ranked paper search with explicit fields and bounded offset/limit. Crossref uses the public works search with a bounded `query.bibliographic`, rows and offset. All three are default-disabled so callers opt into their latency and public rate-limit profiles.

OpenAlex is not included in this slice because its current API requires a key. The repository's engine context does not yet expose provider-scoped secrets, so adding OpenAlex now would either hardcode a credential or couple provider code directly to the Worker environment. A separate credential-plumbing slice is required.

## Supplied Rust and Cloudflare repository assessment

- `cloudflare/boring`: native BoringSSL bindings and Tokio/Hyper TLS adapters. It remains excluded from the Worker runtime because it does not provide a `wasm32-unknown-unknown` Workers Fetch path.
- `cloudflare/wildcard`: useful for wildcard matching, but the current outbound policy uses a small exact compile-time hostname set. Adding it would increase dependency surface without improving current correctness.
- `cloudflare/sliceslice-rs`: AVX2/x86 substring search. It is incompatible with the Worker Wasm target and unnecessary for bounded provider responses.
- `cloudflare/entropy-map`: compact immutable minimal-perfect-hash maps. The eight-entry engine registry and current ranking tables are too small to justify construction complexity or bundle cost; retain as a future option for genuinely large immutable lookup tables.
- `cloudflare/cardinality-estimator`: HyperLogLog++ distinct counting. It is relevant to later telemetry aggregation, not request-time result reranking, and is deferred until per-provider analytics need approximate cardinality.
- `Anush008/fastembed-rs`: local ONNX/Candle embeddings and rerankers. Its model weights, ORT/tokenizer stack, native-oriented download/TLS defaults and runtime footprint do not fit the current Worker Wasm bundle and latency budget. Semantic reranking should use a separately validated Worker-compatible inference path rather than embedding this crate in the request Worker.

These repositories were evaluated as requested. None is added as a dependency in the academic-provider slice because each fails either the target-compatibility, workload-fit, or bundle-value gate.
