# Source Notes

Reviewed: 2026-07-27

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

These sources confirm the Workers Fetch execution model, manual redirect control, preview/deployment behavior, secret handling, `workers-rs` build workflow and `wasm32-unknown-unknown` target. The supported-crates page is explicitly non-exhaustive, so compatibility is established by the exact-target build and workerd tests rather than documentation listings alone.

`lol-html` 3.0.0 remains the required bounded streaming parser for every HTML provider. No browser DOM or native HTML parser was introduced. Mojeek derives its relative date-filter epoch from the existing Worker request deadline instead of adding a clock dependency.

## SearXNG behavioral references

Upstream repository: https://github.com/searxng/searxng

Current files consulted:

- `searx/engines/arxiv.py`
- `searx/engines/brave.py`
- `searx/engines/qwant.py`
- `searx/engines/github.py`
- `searx/engines/mojeek.py`
- `searx/engines/yahoo.py`
- `searx/engines/yandex.py`
- `searx/engines/baidu.py`
- `searx/engines/google.py`
- `searx/engines/grokipedia.py`
- `searx/engines/openalex.py`
- `searx/engines/semantic_scholar.py`
- `searx/engines/startpage.py`
- `searx/engines/wikipedia.py`
- https://docs.searxng.org/user/configured_engines.html

The references were used to identify public endpoints, stable query parameters, result containers, paging conventions, locale/SafeSearch handling, tracking-link formats, provider errors and user-facing bang behavior. Searxflare does not copy SearXNG control flow or Python implementation. Its Rust adapters use the repository's own engine traits, restricted transport, bounded `lol-html` parser, failure taxonomy, caching, query-aware ranking and Worker target constraints.

SearXNG is AGPL-3.0-or-later. These notes preserve provenance and make the clean-room behavioral adaptation explicit. See `spec/licensing.md`.

The standalone request values `github`, `3.0`, and `1.0` were not mapped to Searxflare weight, timeout, version or maturity fields because their intended meaning was not specified.

## Public provider surfaces

- Brave: `https://search.brave.com/search`
- Qwant: `https://api.qwant.com/v3/search/web`
- Mojeek: `https://www.mojeek.com/search`
- Yahoo regional search frontends under a compile-time `*.search.yahoo.com` allow-list
- Yandex: `https://yandex.com/search/`
- Baidu: `https://www.baidu.com/s`
- Google: `https://www.google.com/search`
- Grokipedia: `https://grokipedia.com/api/full-text-search`
- Mojeek request parameters: https://www.mojeek.com/support/api/search/request_parameters.html
- Mojeek search operators: https://www.mojeek.com/support/search-operators.html
- Yahoo regional/language help: https://help.yahoo.com/kb/regional-language-specific-yahoo-search-results-sln6583.html
- Yahoo SafeSearch help: https://help.yahoo.com/kb/search-for-desktop/select-setting-yahoo-safesearch-sln2247.html

These public frontend or search surfaces are provider-controlled contracts. They can change, challenge, rate-limit or deny Cloudflare egress. All newly added providers remain default-disabled. The implementation does not bypass those controls; challenge, denial, changed-layout, schema, rate-limit and empty-result states remain isolated.

Qwant uses the smaller caller limit capped at ten and calculates the page offset from that effective count. Mojeek uses bounded `q`, `safe`, optional `s` and `since` parameters plus fixed locale cookies. Yahoo uses a compile-time regional host map, bounded offsets, stable locale/SafeSearch cookies and day/week/month filters. Yandex, Baidu and Google use bounded public HTML requests and streaming parsing. Baidu tracking result URLs are not followed through a second request. Google and Yahoo tracking wrappers are decoded without network access. Grokipedia uses a bounded JSON request and strict results-array validation.

## Official reference, academic and code APIs

- arXiv API: https://info.arxiv.org/help/api/user-manual.html
- MediaWiki REST API page summary: https://www.mediawiki.org/wiki/API:REST_API/Reference
- NCBI E-utilities: https://www.ncbi.nlm.nih.gov/books/NBK25500/
- Semantic Scholar public search frontend: `https://www.semanticscholar.org/api/1/search`
- Crossref REST API: https://crossref.gitlab.io/rest-api-doc/
- GitHub repository search: https://docs.github.com/en/rest/search/search
- GitHub API versioning: https://docs.github.com/en/rest/about-the-rest-api/api-versions
- GitHub rate limits: https://docs.github.com/en/rest/rate-limit/rate-limit
- GitHub REST best practices: https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api

GitHub uses a fixed host, stable repository-identifying User-Agent, bounded `page`/`per_page`, `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2026-03-10`, strict schema validation and explicit rate-limit classification. It is unauthenticated in this slice because provider-scoped secrets are not yet exposed to engines.

## Exa MCP research

- Reference: https://exa.ai/docs/reference/exa-mcp
- Remote endpoint: `https://mcp.exa.ai/mcp`
- Transport: Streamable HTTP
- Authentication: optional `x-api-key`; the hosted free service works without it

The original public-provider slice deferred Exa because the engine abstraction
did not expose provider-scoped secrets or an MCP lifecycle. The stateful slice
below resolves those boundaries with a fixed-host, fixed-tool Streamable HTTP
adapter. A 429 is honored; rotating identities or headers to evade limits
remains out of scope.

## Stateful, authenticated and experimental providers

The 2026-07-27 stateful slice adds provider-scoped optional secret access without
exposing the raw Worker environment to engines. GitHub uses a token when one is
configured, Crossref adds the operator `mailto`, and OpenAlex uses the public
official `/works?search=` endpoint without requiring an API key. Semantic
Scholar uses the same public web-search request shape as current SearXNG,
including a provider-issued UI version marker read from its homepage.

WolframAlpha is not registered because its official programmable Full Results
API requires an App ID and current SearXNG has no keyless Wolfram engine. Brave
News and Startpage remain clean-room, fixed-host public-frontend adapters and
are default-disabled.

Exa uses the official Streamable HTTP server at `https://mcp.exa.ai/mcp`, the
optional documented `x-api-key` header, and only the documented
`web_search_exa` tool:
https://exa.ai/docs/reference/exa-mcp. The adapter has a fixed two-request
initialize/call lifecycle, a fixed tool name, bounded arguments and no arbitrary
MCP method, tool, host or header surface.

Web Bot Auth signing uses Cloudflare's Apache-2.0 `web-bot-auth` Rust crate and
the published Ed25519 HTTP Message Signatures flow. Private keys remain Worker
secrets; only the public directory and agent card are served.

## Supplied Rust repository assessment

- `cloudflare/boring`: native TLS stack, not a Workers Fetch path.
- `cloudflare/wildcard`: unnecessary for the small exact compile-time hostname sets.
- `cloudflare/sliceslice-rs`: x86/AVX2-oriented and unsuitable for Worker Wasm.
- `cloudflare/entropy-map`: not justified for a fifteen-entry registry.
- `cloudflare/cardinality-estimator`: potentially useful for later aggregate telemetry, not request-time ranking.
- `Anush008/fastembed-rs`: model/runtime footprint and native-oriented inference stack do not fit the current Worker request path.

None was added because it failed the target-compatibility, workload-fit or bundle-value gate.
