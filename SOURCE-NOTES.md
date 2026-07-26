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
- Rust language support: https://developers.cloudflare.com/workers/languages/rust/
- Supported Rust crates: https://developers.cloudflare.com/workers/languages/rust/crates/

These sources were used to confirm the Worker Fetch execution model, preview deployment behavior, secret handling, deployment/version inspection, `workers-rs` execution model, `wasm32-unknown-unknown` target, `worker-build` workflow, and dependency review expectations. The supported-crates page is explicitly non-exhaustive, so dependency compatibility is established by the repository's exact-target build and workerd tests rather than by documentation listings alone. Live account state was checked separately through Cloudflare Code Mode; documentation is not treated as proof of deployment.

`lol-html` 3.0.0 was already present in the workspace and remains the required bounded streaming parser for HTML providers. Mojeek and Yahoo use the shared selector parser, so no browser DOM or native HTML parser was introduced. `js-sys`, already present in the workspace lockfile, supplies the Worker-compatible clock used to build deterministic Mojeek date filters.

## SearXNG behavioral references

Upstream repository: https://github.com/searxng/searxng

Reference commits:

- `0909dbc9efb2c6e93e2ad51e60e66417ab291710`
- `6d8b55028063d2c36dd1b95d43ef1ebf82580698`

Files and documentation consulted:

- `searx/engines/arxiv.py`
- `searx/engines/brave.py`
- `searx/engines/qwant.py`
- `searx/engines/github.py`
- `searx/engines/mojeek.py`
- `searx/engines/yahoo.py`
- https://docs.searxng.org/user/configured_engines.html

The reference was used to identify public endpoints, stable query parameters, result containers, paging limits, safe-search mapping, locale handling, provider error shapes, arXiv namespace handling, Yahoo regional and tracking-URL behavior, Mojeek paging and filter conventions, and user-facing engine/category bang behavior. Searxflare does not copy SearXNG control flow or Python implementation. The Rust adapters use the repository's own engine traits, restricted transport, bounded parsers, failure taxonomy, caching, ranking, and `wasm32-unknown-unknown` constraints.

SearXNG is AGPL-3.0-or-later. These notes preserve provenance and make the clean-room behavioral adaptation explicit. See `spec/licensing.md` for the repository's licensing policy.

The standalone request values `github`, `3.0`, and `1.0` match fields displayed in SearXNG's configured-engine table, but their intended meaning in this repository was not stated. They were deliberately not mapped to Searxflare weight, timeout, version or maturity fields.

## Provider surfaces

- Brave public search frontend: `https://search.brave.com/search`
- Qwant public frontend JSON surface: `https://api.qwant.com/v3/search/web`
- Mojeek public search frontend: `https://www.mojeek.com/search`
- Yahoo public regional search frontends under the compile-time `*.search.yahoo.com` host set
- Mojeek request parameter reference: https://www.mojeek.com/support/api/search/request_parameters.html
- Mojeek search operators: https://www.mojeek.com/support/search-operators.html
- Yahoo regional and language help: https://help.yahoo.com/kb/regional-language-specific-yahoo-search-results-sln6583.html
- Yahoo SafeSearch help: https://help.yahoo.com/kb/search-for-desktop/select-setting-yahoo-safesearch-sln2247.html

Brave, Qwant, Mojeek HTML and Yahoo HTML are provider-controlled public frontend contracts rather than supported unauthenticated APIs. Provider behavior can change, rate-limit, challenge, or deny Cloudflare egress. The implementation does not bypass those controls; failures remain isolated and observable.

Mojeek's official request reference documents `q`, `s`, `since`, `safe`, language and region concepts for its authenticated Search API. The Searxflare adapter does not claim API access and does not send an API key; it uses the public HTML frontend pattern validated by the current SearXNG implementation. Yahoo's official help confirms regional or language filtering where available and the Off, Moderate and Strict SafeSearch levels. The exact frontend cookie and paging mappings remain provider-controlled and were adapted from current SearXNG behavior.

Qwant's request builder now uses the caller's bounded result limit for both `count` and page-offset calculation. Its parser version was bumped so previous cache entries cannot hide the changed request semantics.

## Official reference, academic and code APIs

- arXiv API user manual: https://info.arxiv.org/help/api/user-manual.html
- MediaWiki Action API overview: https://www.mediawiki.org/wiki/API:Action_API
- MediaWiki search module: https://www.mediawiki.org/wiki/API:Search
- MediaWiki API etiquette: https://www.mediawiki.org/wiki/API:Etiquette
- Wikimedia API access policy: https://www.mediawiki.org/wiki/Wikimedia_APIs/Access_policy
- NCBI E-utilities usage and ESearch/ESummary flow: https://www.ncbi.nlm.nih.gov/books/NBK25500/
- Semantic Scholar Academic Graph API: https://api.semanticscholar.org/api-docs/graph
- Crossref REST API: https://crossref.gitlab.io/rest-api-doc/
- OpenAlex API overview and authentication: https://developers.openalex.org/
- GitHub REST repository search: https://docs.github.com/en/rest/search/search
- GitHub REST API versioning: https://docs.github.com/en/rest/about-the-rest-api/api-versions
- GitHub REST rate limits: https://docs.github.com/en/rest/rate-limit/rate-limit
- GitHub REST best practices: https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api

The arXiv manual defines field-qualified Boolean query construction, bounded `start`/`max_results` paging, Atom output, and error responses represented as single Atom entries. Searxflare now qualifies each user term, detects error entries before requiring paper-only fields such as `published`, and preserves upstream error details.

The MediaWiki documentation identifies `/w/api.php` as the programmatic Action API endpoint, supports search as a generator, recommends JSON for new clients, and requires a meaningful client identifier. Wikipedia now uses one bounded generator request for page extracts, canonical URLs and optional thumbnails instead of the human Special:Search HTML surface.

PubMed uses a bounded ESearch JSON request followed by ESummary JSON only when IDs are returned. Semantic Scholar uses relevance-ranked paper search with explicit fields and bounded offset/limit. Crossref uses the public works search with a bounded `query.bibliographic`, rows and offset. All three are default-disabled so callers opt into their latency and public rate-limit profiles.

GitHub uses the public repository-search endpoint with `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2026-03-10`, a stable repository-identifying User-Agent, bounded `page`/`per_page`, strict result-shape validation, and explicit rate-limit classification. It is unauthenticated in this first slice because the engine context does not yet expose provider-scoped secrets. It remains default-disabled until Cloudflare preview egress and observed limit behavior are validated. No client impersonation, random header rotation, blind retry or limit evasion is implemented.

OpenAlex is not included in this slice because its current API requires a key. The repository's engine context does not yet expose provider-scoped secrets, so adding OpenAlex now would either hardcode a credential or couple provider code directly to the Worker environment. A separate credential-plumbing slice is required.

## Exa MCP research

- Exa MCP reference: https://exa.ai/docs/reference/exa-mcp
- Remote endpoint: `https://mcp.exa.ai/mcp`
- Transport documented by Exa: Streamable HTTP
- Production authentication: `x-api-key`

Exa's remote MCP endpoint can be reached by MCP clients over Streamable HTTP and supports an optional API key for production usage and higher limits. The current Searxflare engine abstraction models bounded search-provider HTTP requests and does not implement MCP initialization, capability negotiation, tool discovery or tool invocation. Exa is therefore not added to the engine registry in this slice. The next Exa slice must add provider-scoped secret plumbing and either a narrowly bounded MCP transport adapter or an explicitly approved direct Exa API adapter. A 429 must be honored; rotating identities or headers to evade the free-plan limit is out of scope.

## Supplied Rust and Cloudflare repository assessment

- `cloudflare/boring`: native BoringSSL bindings and Tokio/Hyper TLS adapters. It remains excluded from the Worker runtime because it does not provide a `wasm32-unknown-unknown` Workers Fetch path.
- `cloudflare/wildcard`: useful for wildcard matching, but the current outbound policy uses a small exact compile-time hostname set. Adding it would increase dependency surface without improving current correctness.
- `cloudflare/sliceslice-rs`: AVX2/x86 substring search. It is incompatible with the Worker Wasm target and unnecessary for bounded provider responses.
- `cloudflare/entropy-map`: compact immutable minimal-perfect-hash maps. The eleven-entry engine registry and current ranking tables are too small to justify construction complexity or bundle cost; retain as a future option for genuinely large immutable lookup tables.
- `cloudflare/cardinality-estimator`: HyperLogLog++ distinct counting. It is relevant to later telemetry aggregation, not request-time result reranking, and is deferred until per-provider analytics need approximate cardinality.
- `Anush008/fastembed-rs`: local ONNX/Candle embeddings and rerankers. Its model weights, ORT/tokenizer stack, native-oriented download/TLS defaults and runtime footprint do not fit the current Worker Wasm bundle and latency budget. Semantic reranking should use a separately validated Worker-compatible inference path rather than embedding this crate in the request Worker.

These repositories were evaluated as requested. None is added as a dependency in this slice because each fails either the target-compatibility, workload-fit, or bundle-value gate.
