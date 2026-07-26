# Searxflare

Searxflare is an API-only metasearch engine written in Rust for Cloudflare Workers. It compiles to `wasm32-unknown-unknown`, queries fixed public provider endpoints through Workers Fetch, normalises and deduplicates results, and returns deterministic JSON.

The canonical system design is [`architecture.md`](architecture.md). Public contracts live under [`spec/`](spec/).

## Implemented production slices

- `workers-rs` Router entrypoint
- bearer API-key authentication for `/v1/*`
- RFC 9457-style problem responses and request IDs
- `/healthz`, `/readyz`, `/v1/search`, engine catalogue/debug routes and SearXNG-compatible JSON route
- compile-time engine registry
- restricted HTTPS-only outbound transport with host allow-lists, manual redirects, deadlines, bounded bodies, content-type checks and challenge classification
- Cache API for aggregate, per-engine and negative responses
- stateless and KV engine-state adapters plus a Durable Object coordinator contract
- arXiv Atom, Wikipedia Action API JSON, DuckDuckGo HTML, Brave Web HTML, Qwant Web JSON, PubMed JSON, Semantic Scholar JSON and Crossref JSON adapters
- four default-enabled adapters, including DuckDuckGo and Brave as independent general-web providers
- Unicode NFC query normalisation, URL canonicalisation, exact canonical-URL deduplication
- `rrf-v1` and `searx-compat-v1` ranking
- structured logs and optional Analytics Engine events

Brave completes the canonical architecture's Milestone 5 default provider-diversity goal. Qwant is implemented and available for explicit requests, but remains default-disabled because its public frontend challenged Cloudflare preview egress during delivery validation. Each provider has a fixed host allow-list, bounded request construction, fixture-backed parsing and independent partial-failure semantics. Their public frontend contracts remain provider-controlled and may change without notice.

arXiv uses its official Atom API with provider-safe multi-term query construction and explicit Atom error detection. Wikipedia uses the official MediaWiki Action API rather than the human Special:Search HTML surface, avoiding frontend challenge pages while preserving bounded paging, locale selection, extracts, canonical URLs and thumbnails.

PubMed, Semantic Scholar and Crossref add explicit academic search without changing the default general-web fan-out. They are default-disabled so callers opt into their provider-specific latency and rate-limit profiles. PubMed uses a bounded two-request ESearch-to-ESummary flow; Semantic Scholar and Crossref each use one JSON request.

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
```

Generate a hash:

```bash
printf '%s' 'replace-with-a-long-random-key' | shasum -a 256
```

Run:

```bash
npm run dev
```

Example:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=cloudflare+rust'
```

Explicitly request all registered general-web providers, including default-disabled Qwant:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=cloudflare&engines=duckduckgo-html,brave-web,qwant-web'
```

Explicitly request the specialized academic providers:

```bash
curl -H 'Authorization: Bearer replace-with-a-long-random-key' \
  'http://127.0.0.1:8787/v1/search?q=food+safety&engines=pubmed,semantic-scholar,crossref'
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
- [Milestone 5 implementation checklist](spec/milestone-5-checklist.md)
- [Academic provider implementation checklist](spec/academic-provider-checklist.md)
- [Source and provenance notes](SOURCE-NOTES.md)
- [Licence and provenance](spec/licensing.md)
