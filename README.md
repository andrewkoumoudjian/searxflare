# Searxflare

Searxflare is a metasearch engine written in Rust for Cloudflare Workers. It compiles to `wasm32-unknown-unknown`, queries fixed public provider endpoints through Workers Fetch, normalises and deduplicates results, reranks the combined set against the user's original search query, returns deterministic JSON, and serves a small same-origin search interface at `/`.

The canonical system design is [`architecture.md`](architecture.md). Public contracts live under [`spec/`](spec/).

## Implemented production slices

- `workers-rs` Router entrypoint
- bearer API-key authentication for every search and `/v1/*` route
- RFC 9457-style problem responses and request IDs
- `/healthz`, `/readyz`, `/v1/search`, engine catalogue/debug routes and SearXNG-compatible JSON route
- compile-time engine and bang registries
- restricted HTTPS-only outbound transport with host allow-lists, manual redirects, deadlines, bounded bodies, content-type checks and challenge classification
- Cache API for aggregate, per-engine and negative responses
- stateless and KV engine-state adapters plus a sharded Durable Object provider coordinator
- per-client Workers Rate Limiting, signed public cursors and shared provider cooldowns
- provider-scoped credentials and optional Web Bot Auth request signing
- engine-agnostic public results with provider metadata namespaced by engine ID
- deterministic `query-aware-v1` reranking after normalization and canonical-URL deduplication
- explicit `rrf-v1` and `searx-compat-v1` compatibility ranking modes
- SearXNG-style compile-time bangs for engine, category and profile selection
- structured logs and optional Analytics Engine events
- a public Google-inspired UI backed by the separately rate-limited `/ui/search` route
- bounded post-response page capture into R2 with a KV-backed crawl frontier
- optional Cloudflare AI Search retrieval using hybrid vector/BM25 search and reciprocal rank fusion

## Engine catalogue

The compile-time catalogue contains twenty engines:

- arXiv Atom
- Wikipedia Action API JSON
- DuckDuckGo HTML
- Brave Web HTML
- Brave News HTML
- Qwant Web JSON
- PubMed JSON
- Semantic Scholar JSON
- Crossref JSON
- GitHub REST repository search
- Mojeek HTML
- Yahoo HTML
- Yandex HTML
- Baidu HTML
- Google HTML
- Grokipedia JSON
- WolframAlpha Full Results JSON
- OpenAlex Works JSON
- Exa Streamable HTTP MCP
- Startpage HTML

All registered engines are selected when no engines or categories are specified. Google and Startpage remain controlled by `ENABLE_GOOGLE` and `ENABLE_STARTPAGE`, which are enabled in the production Worker configuration. Engines without available credentials or upstream access fail independently and are surfaced through partial-result metadata.

All HTML adapters use the repository's bounded streaming `lol-html` parser. They do not execute a browser, solve CAPTCHAs, rotate proxies, impersonate random clients, or accept user-defined destinations. Challenge, denial, changed-layout, rate-limit and empty-result states are classified independently.

Qwant honors smaller caller limits while capping requests at its ten-result page size. Mojeek supports bounded paging, locale/region cookies, SafeSearch and relative date filters. Yahoo uses a compile-time regional host map, bounded paging, language/SafeSearch cookies and day/week/month filters. Yandex, Baidu and Google are bounded, fixed-host HTML adapters; Google also maps locale, SafeSearch and time-range parameters. Grokipedia uses a bounded JSON request and strict result-array validation.

GitHub repository search is available through explicit engine selection or `!gh`. It uses the public REST repository-search endpoint with a fixed host, bounded paging, stable client identification and rate-limit classification. The standalone request values `github`, `3.0` and `1.0` were not interpreted as weights, versions or timeouts.

Exa MCP uses a fixed two-request Streamable HTTP lifecycle and only invokes `web_search_exa`; its host, method, tool and arguments remain bounded. OpenAlex, WolframAlpha and Exa require their provider-scoped secrets. No header rotation or rate-limit evasion is implemented.

## Query selection and ranking

Recognized bangs are parsed before final query normalization and removed from the query sent to providers. The normalized original query and provider query are returned in response metadata.

Supported aliases include:

- `!web` and `!general` for the general category
- `!books` for the current books profile
- `!gh` / `!github`
- `!wp` / `!wikipedia`
- `!ax` / `!arxiv`
- `!mj` / `!mojeek`
- `!yh` / `!yahoo`
- `!ya` / `!yandex`
- `!bd` / `!baidu`
- `!google`
- `!news` / `!brave-news`
- `!wa` / `!wolfram`
- `!openalex`
- `!exa`
- `!sp` / `!startpage`
- `!grok` / `!grokipedia`
- `!ddg`, `!brave`, `!qw`, `!qwant`, `!pubmed`, `!ss`, `!semantic-scholar`, `!cr` and `!crossref`

Multiple engine bangs may be combined, as may compatible category aliases. Mixing an engine bang with a category bang is rejected deterministically. Unknown bangs and bang-only queries return `INVALID_REQUEST`. Prefix a bang with `\` to keep it as literal query text.

After every provider result is normalized and exact canonical-URL duplicates are merged, `query-aware-v1` scores the combined set using title and content relevance, exact phrase and token coverage, provider position and weight, independent-engine support, canonical-URL terms and bounded relative freshness. Cloudflare's `entropy-map` provides compact exact-token membership and `wildcard` handles normalized prefix matches. The ranking remains deterministic and model-free, so it runs within the Worker request path.

Successful UI and API searches schedule a bounded crawl after the response. The facade uses Cloudflare's streaming HTMLRewriter, which is backed by `lol-html`, captures text and outbound HTTPS links, stores one Markdown document per page in `searxflare-crawl-documents`, and records discovered links in `CRAWL_STATE`. A scheduled handler drains four frontier URLs every 30 minutes. When the optional `CRAWL_SEARCH` binding is active, its results join the provider set using hybrid retrieval with RRF; query rewriting and reranking stay disabled and each query requests at most five chunks.

## Prerequisites

- Node.js 22 or newer
- Python 3 for schema validation
- `curl` for automatic Rust installation when the pinned toolchain is absent

Rust 1.97.1, the `wasm32-unknown-unknown` target and `worker-build` 0.8.5 are pinned by the repository. `scripts/build_worker.sh` installs missing Rust build tools and always validates `Cargo.lock` before compiling.

Install the locked JavaScript dependencies:

```bash
npm ci
```

## Build and test

```bash
cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace
cargo build --locked --workspace --target wasm32-unknown-unknown
npm run validate:specs
npm test
npm run bundle
npm run bundle:size
```

`npm test` builds the Worker and runs the integration suite inside Cloudflare's workerd-based Vitest pool. `npm run bundle` performs a Wrangler dry-run into `dist/`.

## Local development

Create `.dev.vars` and never commit it:

```text
API_KEY_SHA256=<lowercase SHA-256 hex of the bearer key>
CURSOR_SIGNING_KEY=<long random signing secret>
```

Generate a hash:

```bash
printf '%s' 'replace-with-a-long-random-key' | shasum -a 256
```

Run:

```bash
npm run dev
```

Default query-aware search:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=cloudflare+rust'
```

Select the general category:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=!web+cloudflare+rust'
```

Search GitHub repositories:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=!gh+cloudflare+workers+rust'
```

Combine explicit public providers:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=!mj+!yh+!ya+independent+search'
```

Explicitly request a challenge-prone provider:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=cloudflare&engines=google-web'
```

## Deployment

See [`docs/deployment.md`](docs/deployment.md) for Wrangler and Cloudflare Git integration settings. A local endpoint working does not prove that the same provider permits Cloudflare production egress.

## Operational boundaries

Searxflare queries public endpoints without paid search APIs. It does not bypass CAPTCHAs or access controls, execute browsers, rotate proxies, impersonate random browsers, or accept user-defined engines. Provider blocks are classified and returned as partial failures. Cache API entries are local to a Cloudflare PoP, and Cloudflare Access must not front a route that depends on Cache API.

## Documentation

- [Engine implementation guide](docs/engine-guide.md)
- [Fixture capture guide](docs/fixture-capture.md)
- [Security assumptions](docs/security.md)
- [Known limitations](docs/known-limitations.md)
- [Provider disable procedure](docs/provider-disable.md)
- [Source and provenance notes](SOURCE-NOTES.md)
- [Licence and provenance](spec/licensing.md)
