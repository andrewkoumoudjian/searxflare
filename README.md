# Searxflare

Searxflare is an API-only metasearch engine written in Rust for Cloudflare Workers. It compiles to `wasm32-unknown-unknown`, queries fixed public provider endpoints through Workers Fetch, normalises and deduplicates results, and returns deterministic JSON.

The canonical system design is [`architecture.md`](architecture.md). Public contracts live under [`spec/`](spec/).

## Initial production slice

- `workers-rs` Router entrypoint
- bearer API-key authentication for `/v1/*`
- RFC 9457-style problem responses and request IDs
- `/healthz`, `/readyz`, `/v1/search`, engine catalogue/debug routes and SearXNG-compatible JSON route
- compile-time engine registry
- restricted HTTPS-only outbound transport with host allow-lists, manual redirects, deadlines, bounded bodies, content-type checks and challenge classification
- Cache API for aggregate, per-engine and negative responses
- stateless and KV engine-state adapters plus a Durable Object coordinator contract
- arXiv Atom, Wikipedia HTML and DuckDuckGo HTML adapters
- Unicode NFC query normalisation, URL canonicalisation, exact canonical-URL deduplication
- `rrf-v1` and `searx-compat-v1` ranking
- structured logs and optional Analytics Engine events

## Prerequisites

- Rust 1.97.1 with `wasm32-unknown-unknown`
- Node.js 22 or newer
- `worker-build` 0.8.5
- Wrangler 4

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build --version 0.8.5 --locked
npm install
```

## Build and test

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --target wasm32-unknown-unknown
npm run validate:specs
npm test
npm run bundle
```

## Local development

Create `.dev.vars` (never commit it):

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

## Deployment

See [`docs/deployment.md`](docs/deployment.md). A local endpoint working does not prove that the same provider permits Cloudflare production egress.

## Operational boundaries

Searxflare queries public endpoints without paid search APIs. It does not bypass CAPTCHAs or access controls, execute browsers, rotate proxies, impersonate random browsers, or accept user-defined engines. Provider blocks are classified and returned as partial failures. Cache API entries are local to a Cloudflare PoP, and Cloudflare Access must not front a route that depends on Cache API.

## Documentation

- [Engine implementation guide](docs/engine-guide.md)
- [Fixture capture guide](docs/fixture-capture.md)
- [Security assumptions](docs/security.md)
- [Known limitations](docs/known-limitations.md)
- [Provider disable procedure](docs/provider-disable.md)
- [Licence and provenance](spec/licensing.md)
