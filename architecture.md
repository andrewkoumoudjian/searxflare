# Serverless Rust Metasearch on Cloudflare Workers

## Verified Architecture and Specification-Driven Implementation Plan

## 1. Executive verdict

Build an **API-only metasearch engine in Rust**, compiled to `wasm32-unknown-unknown` and deployed as a Cloudflare Worker.

The service will query the same classes of open endpoints that SearXNG uses:

* Public HTML search pages
* Public JSON endpoints used by search frontends
* Atom, RSS and XML search feeds
* HTML containing embedded result data
* Multi-step endpoints requiring transient tokens
* Search endpoints whose continuation URL must be discovered from an earlier response

The MVP will not depend on:

* Paid search API services
* Browser Run
* Headless browsers
* Containers
* A permanently running server
* External databases
* LLM ranking
* CAPTCHA solving
* Proxy rotation
* User-supplied engine code

The default outbound transport will be **Cloudflare Workers Fetch**. `reqwest` may be used as an ergonomic Rust facade because its Wasm build delegates to JavaScript `fetch`, but it does not restore native `reqwest` features such as custom TLS, SOCKS, HTTP/3, connection pools or native compression. Cloudflare explicitly confirms that `reqwest` uses JavaScript `fetch` on Workers, while only Hyper’s lower-level `conn` interface can operate over a Workers socket.

The MVP architecture will use:

```text
workers-rs
+ Workers Fetch
+ Cache API
+ KV
+ narrowly scoped Durable Objects
+ serde_json
+ quick-xml
+ lol-html
+ Analytics Engine
+ Workers Logs and traces
+ API Shield OpenAPI validation
+ mTLS, JWT or Worker API keys
+ optional Web Bot Auth
```

## 2. Corrections to previous assumptions

### 2.1 Cloudflare Access and Cache API cannot front the same search route

The prior diagram placed Cloudflare Access before a Worker using Cache API. Cloudflare currently states that Cache API operations are unavailable for Workers fronted by Access. Cache entries are also local to the data centre where they were created and do not replicate globally.

The corrected edge policy is:

```text
/v1/search
├── mTLS, JWT or Worker API key
├── API Shield
└── Cache API available

/admin/*
├── Cloudflare Access
└── no dependency on Cache API
```

### 2.2 `reqwest` works, but it is not a separate transport

Cloudflare confirms that `reqwest` compiles to Wasm and automatically uses JavaScript `fetch`.

Cloudflare’s `cf-reqwest` also has a Wasm target, but its native transport features are separated behind `cfg(not(target_arch = "wasm32"))`. Rustls, native TLS, SOCKS, HTTP/2, HTTP/3 and custom connectors are therefore not available through its Wasm implementation. The Wasm path uses `web-sys` request, response, stream and abort primitives.

Therefore:

```text
worker::Fetch       ─┐
reqwest on Wasm     ─┼── Cloudflare Fetch transport
cf-reqwest on Wasm  ─┘
```

Use `reqwest` only if it makes Websurfx-inspired adapters easier to port and does not materially increase the Worker bundle.

### 2.3 Chaussette is relevant, but cannot be embedded in the Worker

Chaussette is Cloudflare’s SOCKS5-to-Privacy-Proxy client. It accepts local SOCKS5 connections and forwards them through authenticated HTTP CONNECT tunnels over HTTP/2 or HTTP/3, with optional geohash-based egress selection.

Its implementation assumes:

* A Tokio TCP listener
* Inbound SOCKS5 sessions
* BoringSSL
* Hyper
* HTTP/2
* Tokio-Quiche
* A native process lifecycle

Workers cannot accept inbound TCP connections today, and Chaussette’s native dependencies do not target the Worker runtime.

Cloudflare Privacy Proxy is nevertheless a legitimate future egress option. It accepts authenticated CONNECT tunnels over HTTP/2 or HTTP/3 and provides managed Cloudflare egress. It is currently an Enterprise-managed product requiring provisioning.

A Worker-native Privacy Proxy client is **not established by the current documentation**:

* Workers Fetch does not expose a usable CONNECT tunnel stream.
* Raw Workers sockets cannot connect to Cloudflare IP ranges.
* Privacy Proxy requires HTTP/2 or HTTP/3 CONNECT.
* Workers do not expose outbound UDP for an HTTP/3 client.
* Hyper’s lower-level connection API supports Workers sockets, but a direct socket to a Cloudflare Privacy Proxy endpoint may be blocked.

The correct decision is:

```text
MVP:             no Privacy Proxy dependency
Post-MVP spike:  test Worker → Privacy Proxy feasibility
Fallback:        separately operated Chaussette/native relay
```

### 2.4 Rustls is not impossible, but it is not a proven Worker dependency

Rustls is a platform-independent TLS 1.2 and TLS 1.3 implementation requiring a cryptographic provider and an underlying byte stream.

Its maintainers have discussed `wasm32-unknown-unknown`, and applications may enable the appropriate Ring Wasm feature directly. However, proposals to formalise a Rustls web/Wasm feature were closed without merge, with maintainers noting that complete Wasm support required representative examples and CI.

A Rustls-over-Workers-Socket experiment is theoretically possible:

```text
Workers Socket
    ↓
Rustls client connection
    ↓
Hyper conn
    ↓
HTTP/1.1 or HTTP/2
```

But it is not an MVP dependency because it would require:

* A proven Wasm crypto provider
* Root certificate management
* Secure randomness
* Socket adapters
* TLS I/O state machines
* Hyper integration
* Compression support
* Redirect support
* A significant bundle-size increase

It would still not change the Worker’s Cloudflare egress identity or provide SOCKS, Tor or arbitrary source-IP selection.

### 2.5 Durable state is necessary for selected engines

The Worker orchestration remains stateless, but some engines are not.

SearXNG’s engine cache stores:

* DuckDuckGo query-dependent continuation data
* Startpage tokens
* WolframAlpha codes
* SoundCloud guest credentials
* Other transient provider state

The correct model is:

```text
Stateless Worker
├── query planning
├── fan-out
├── parsing
├── aggregation
└── response

State services
├── Cache API: local search responses
├── KV: slow-changing traits and snapshots
└── Durable Objects: exact token refresh and cooldown mutation
```

## 3. Verified reference hierarchy

### 3.1 SearXNG: authoritative behaviour reference

Use SearXNG for:

* Endpoint selection
* Request parameters
* Cookies
* Headers
* Locale and country mappings
* Safe-search mappings
* Time-range mappings
* Pagination
* Transient tokens
* Continuation URLs
* Redirect decoding
* Result fields
* Challenge detection
* Error semantics
* Engine suspension
* Result merging

SearXNG’s online processor establishes a clear engine lifecycle:

```text
default request parameters
    ↓
engine.request(query, params)
    ↓
network GET or POST
    ↓
redirect and HTTP handling
    ↓
engine.response(response)
    ↓
normalized EngineResults
```

Its result container merges duplicate results, preserves all provider positions, calculates aggregate scores and records unresponsive engines.

### 3.2 Websurfx: Rust design reference

Use Websurfx for:

* Rust engine traits
* Typed provider response models
* Compile-time engine registration
* CSS-selector parser configuration
* Per-engine errors
* Concurrent result aggregation
* Fixture-driven parser tests

Its generic parser separates selectors for:

* No-result indicator
* Result container
* Title
* URL
* Description

Its Qwant implementation demonstrates a particularly useful pattern: typed Serde models distinguish successful and failed responses, discard unrelated result categories and are tested against static fixtures.

Do not carry over Websurfx’s native server assumptions:

* Actix
* Tokio multithreaded runtime
* Reqwest native TLS
* SOCKS
* Rayon
* Filesystem access
* Redis
* Moka
* Native allocators
* LuaJIT

### 3.3 Freighter: modularity reference

Freighter is a modular native Rust registry rather than a Worker project. Its useful contribution is architectural: keep authentication, index, storage and transport behind explicit provider traits. Its native Hyper, Tokio, PostgreSQL and object-storage implementation is not Worker-loadable.

Apply that modularity to:

```text
HttpTransport
EngineState
QueryCache
ProviderRegistry
ResultRanker
Authenticator
TelemetrySink
```

### 3.4 Networkquality-rs: transport abstraction reference

Networkquality-rs separates network and time abstractions and can layer proxy networks around another transport. Its current implementation still depends on Tokio, Hyper and native TLS, but the abstract transport design is useful.

### 3.5 Cloudflare repositories: runtime implementation references

Directly useful:

* `workers-rs`
* `lol-html`
* `web-bot-auth`
* `wasm-coredump`
* `wildcard`, if hostname patterns become complex

Useful as design references:

* `freighter`
* `networkquality-rs`
* `wirefilter`
* `saffron`
* `chaussette`
* `cf-reqwest`
* `rustls`

Not suitable as Worker runtime dependencies:

* Pingora
* Quiche
* BoringTun
* BoringSSL bindings
* Foundations
* mmap-sync
* shellflip
* ecdysis
* LazyHTML
* Chaussette itself
* native proxy, media, VPN, process-management and Linux-specific projects

## 4. Corrected production architecture

```text
                            API clients
                                 │
          ┌──────────────────────┴──────────────────────┐
          │ Cloudflare WAF                              │
          │ API Shield OpenAPI 3.0 validation           │
          │ mTLS / JWT / Worker API-key authentication  │
          │ Request rate limits                         │
          └──────────────────────┬──────────────────────┘
                                 │
                                 ▼
                       Rust Metasearch Worker
                                 │
      ┌──────────────────────────┼──────────────────────────┐
      │                          │                          │
      ▼                          ▼                          ▼
 API transport              Query cache                Telemetry
 workers-rs Router          Cache API                  Workers Logs
 JSON request models        local to PoP               Workers Traces
 auth middleware            parsed responses           Analytics Engine
 OpenAPI errors             negative caching
      │
      ▼
 Query normalizer
      │
      ▼
 Engine planner
      ├── resolves categories
      ├── resolves locale and country
      ├── filters disabled engines
      ├── checks provider-state snapshot
      ├── limits active fan-out
      ├── allocates connection slots
      ├── allocates per-engine budgets
      └── establishes total deadline
      │
      ▼
 Concurrent engine executor
      │
      ├── arXiv Atom
      ├── Wikipedia HTML
      ├── Brave HTML
      ├── DuckDuckGo HTML / generated JSON
      ├── Qwant public JSON
      ├── WolframAlpha token / JSON
      ├── Startpage HTML
      └── Google HTML, disabled by default
      │
      ▼
 Restricted EngineHttpClient
      ├── Workers Fetch transport
      ├── compile-time hostname allow-list
      ├── method validation
      ├── parameter encoding
      ├── explicit headers and cookies
      ├── optional Web Bot Auth
      ├── manual redirects
      ├── AbortController deadlines
      ├── bounded response streams
      ├── content-type validation
      └── challenge/error classification
      │
      ▼
 Parser layer
      ├── serde_json
      ├── quick-xml
      ├── lol-html
      ├── optional DOM parser
      └── bounded embedded-data extraction
      │
      ▼
 Normalization
      │
      ▼
 URL canonicalization
      │
      ▼
 Exact deduplication
      │
      ▼
 Ranking and fusion
      │
      ▼
 Stable JSON API response
```

### State plane

```text
Isolate memory
├── immutable engine registry
├── compiled selector specifications
├── locale tables
└── short-lived provider snapshot

Cache API
├── final search response
├── normalized per-engine output
└── short negative cache

Workers KV
├── engine-trait snapshots
├── parser versions
├── provider-state snapshot
└── non-critical expiring tokens

ProviderCoordinator Durable Objects
├── single-flight token refresh
├── atomic cooldown mutation
├── failure counters
├── exact expiry
└── continuation state when an opaque client cursor is insufficient

Secrets / Secrets Store
├── API-key hashes
├── cursor-signing key
├── Web Bot Auth Ed25519 private key
└── optional provider credentials for endpoints that later require them
```

## 5. Cloudflare edge configuration

### Search API

Use:

* Custom Worker domain
* WAF
* API Shield schema validation
* mTLS, JWT or Worker-level API keys
* Workers Rate Limiting
* Cache API

API Shield accepts OpenAPI 3.0 schemas in JSON or YAML. OpenAPI 3.1 is not supported. Start schema validation in log mode before switching endpoints to block mode.

Cloudflare-managed mTLS certificate authorities are available across plans and can authenticate machine clients before requests reach the Worker.

### Administrative API

Use Cloudflare Access for:

```text
/admin/engines
/admin/providers
/admin/canaries
/admin/cache
/admin/debug
/admin/config
```

These routes must not depend on Cache API.

### Rate limiting

Use Workers Rate Limiting for approximate abuse prevention:

```text
/v1/search:
  60 requests per minute per API identity

/v1/engines/{engine}/search:
  15 requests per minute per API identity

/admin/*:
  Access policy plus stricter limits
```

Do not use the Rate Limiting binding for exact billing or provider-credit accounting because it is intentionally permissive and local to a Cloudflare location.

## 6. Rust workspace

```text
metasearch/
├── Cargo.toml
├── Cargo.lock
├── wrangler.jsonc
├── rust-toolchain.toml
├── LICENSE
├── NOTICE
│
├── crates/
│   ├── metasearch-worker/
│   │   ├── src/lib.rs
│   │   ├── src/routes.rs
│   │   ├── src/auth.rs
│   │   ├── src/bindings.rs
│   │   ├── src/health.rs
│   │   └── src/telemetry.rs
│   │
│   ├── metasearch-api/
│   │   ├── src/request.rs
│   │   ├── src/response.rs
│   │   ├── src/problem.rs
│   │   └── src/searxng_compat.rs
│   │
│   ├── metasearch-core/
│   │   ├── src/query.rs
│   │   ├── src/result.rs
│   │   ├── src/engine.rs
│   │   ├── src/planner.rs
│   │   ├── src/executor.rs
│   │   ├── src/canonicalize.rs
│   │   ├── src/deduplicate.rs
│   │   ├── src/rank.rs
│   │   └── src/cursor.rs
│   │
│   ├── metasearch-http/
│   │   ├── src/client.rs
│   │   ├── src/worker_fetch.rs
│   │   ├── src/reqwest_fetch.rs
│   │   ├── src/socket_lab.rs
│   │   ├── src/policy.rs
│   │   ├── src/redirects.rs
│   │   ├── src/cookies.rs
│   │   ├── src/body_limit.rs
│   │   ├── src/challenges.rs
│   │   └── src/web_bot_auth.rs
│   │
│   ├── metasearch-state/
│   │   ├── src/cache.rs
│   │   ├── src/kv.rs
│   │   ├── src/provider_snapshot.rs
│   │   ├── src/provider_coordinator.rs
│   │   └── src/token.rs
│   │
│   ├── metasearch-parsers/
│   │   ├── src/json.rs
│   │   ├── src/xml.rs
│   │   ├── src/html_stream.rs
│   │   ├── src/html_selectors.rs
│   │   ├── src/embedded_data.rs
│   │   ├── src/entities.rs
│   │   └── src/text.rs
│   │
│   └── metasearch-engines/
│       ├── src/registry.rs
│       ├── src/arxiv.rs
│       ├── src/wikipedia.rs
│       ├── src/brave.rs
│       ├── src/duckduckgo_html.rs
│       ├── src/duckduckgo_web.rs
│       ├── src/qwant.rs
│       ├── src/wolframalpha.rs
│       ├── src/startpage.rs
│       └── src/google.rs
│
├── spec/
│   ├── openapi.yaml
│   ├── engine-contract.md
│   ├── engine-manifest.schema.json
│   ├── result.schema.json
│   ├── error.schema.json
│   ├── transport.md
│   ├── caching.md
│   ├── provider-state.md
│   ├── ranking.md
│   ├── canonicalization.md
│   ├── pagination.md
│   ├── security.md
│   ├── licensing.md
│   └── engines/
│       ├── arxiv.yaml
│       ├── wikipedia.yaml
│       ├── brave.yaml
│       ├── duckduckgo-html.yaml
│       ├── duckduckgo-web.yaml
│       ├── qwant.yaml
│       ├── wolframalpha.yaml
│       ├── startpage.yaml
│       └── google.yaml
│
├── fixtures/
│   └── engines/
│
├── tests/
│   ├── contract/
│   ├── integration/
│   ├── differential/
│   └── live/
│
└── scripts/
    ├── validate-specs.sh
    ├── capture-fixture.sh
    ├── run-provider-canary.sh
    ├── build-worker.sh
    └── upload-openapi.sh
```

## 7. Dependency policy

### Required

```toml
worker = { version = "0.8", features = ["http"] }
worker-macros = { version = "0.8", features = ["http"] }

serde = { version = "1", features = ["derive"] }
serde_json = "1"
quick-xml = { version = "0.38", features = ["serialize"] }

futures = "0.3"
async-trait = "0.1"

url = "2"
percent-encoding = "2"
unicode-normalization = "0.1"

thiserror = "2"
time = { version = "0.3", features = ["wasm-bindgen"] }

tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["time"] }
tracing-web = "0.1"

sha2 = "0.10"
hmac = "0.12"
base64 = "0.22"

lol_html = "2"
```

Cloudflare specifically documents the `wasm-bindgen` feature for `time` and the `tracing-web` integration. It also notes that non-I/O spans may show identical start and end times because of Worker timing restrictions.

### Compile-gated

```toml
reqwest = {
  version = "0.12",
  default-features = false,
  features = ["json"],
  optional = true
}

scraper = {
  version = "...",
  default-features = false,
  optional = true
}

hyper = {
  version = "1",
  default-features = false,
  features = ["client", "http1"],
  optional = true
}
```

### Explicitly excluded from the production feature set

```text
tokio multithreaded runtime
tokio filesystem
actix-web
rayon
redis
moka
mimalloc
native reqwest TLS
reqwest SOCKS
reqwest HTTP/3
rustls
quiche
pingora
boring
boringtun
chaussette
foundations
mmap-sync
shellflip
ecdysis
```

## 8. Routing framework

Use `workers-rs::Router` for the MVP.

`workers-rs` supports both its native router and standard `http` types that can integrate with Axum. The API surface here is small, so Axum is unnecessary unless later middleware requirements justify its added bundle size.

Routes:

```text
GET  /healthz
GET  /readyz

GET  /v1/search
POST /v1/search

GET  /v1/engines
GET  /v1/engines/{engine_id}
GET  /v1/engines/{engine_id}/search

GET  /search
GET  /.well-known/http-message-signatures-directory
GET  /signature-agent-card
```

## 9. Engine contract

The engine abstraction must support multi-stage requests.

```rust
#[async_trait::async_trait(?Send)]
pub trait SearchEngine {
    fn descriptor(&self) -> &'static EngineDescriptor;

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure>;
}
```

The future is `?Send` because Worker futures may contain JavaScript-backed objects that are not transferable between threads.

### Engine context

```rust
pub struct EngineContext<'a> {
    pub http: &'a dyn EngineHttpClient,
    pub state: &'a dyn EngineState,
    pub deadline: Deadline,
    pub request_id: &'a str,
}
```

The engine receives no raw `Env`.

It cannot:

* Read arbitrary secrets
* Call arbitrary hosts
* Open a TCP socket
* Bypass body limits
* Write arbitrary KV keys
* Disable redirect validation
* Access another engine’s state

### Engine descriptor

```rust
pub struct EngineDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,

    pub categories: &'static [SearchCategory],
    pub source_kind: SourceKind,
    pub maturity: EngineMaturity,

    pub allowed_hosts: &'static [&'static str],
    pub capabilities: EngineCapabilities,

    pub timeout_ms: u32,
    pub max_body_bytes: usize,
    pub max_steps: u8,
    pub max_redirects: u8,

    pub weight: f64,
    pub parser_version: u16,
    pub default_enabled: bool,

    pub state_policy: StatePolicy,
    pub cache_policy: CachePolicy,
    pub bot_auth_policy: BotAuthPolicy,
}
```

```rust
pub enum SourceKind {
    PublicHtml,
    PublicJson,
    PublicXml,
    HybridHtmlJson,
    MultiStage,
}

pub enum EngineMaturity {
    Stable,
    Beta,
    Experimental,
    Disabled,
}
```

## 10. HTTP transport

### 10.1 Production transport

```rust
#[async_trait::async_trait(?Send)]
pub trait EngineHttpClient {
    async fn send(
        &self,
        engine: &'static EngineDescriptor,
        request: EngineRequest,
        deadline: Deadline,
    ) -> Result<BoundedResponse, EngineFailure>;
}
```

`WorkerFetchClient` is the only required implementation.

`workers-rs` already demonstrates:

* Outbound GET and POST
* JSON decoding
* AbortController
* Fetch cancellation
* Deadline races
* Response cloning

### 10.2 Request model

```rust
pub struct EngineRequest {
    pub method: HttpMethod,
    pub url: Url,
    pub headers: HeaderMap,
    pub cookies: Vec<CookiePair>,
    pub body: Option<Vec<u8>>,
    pub redirect_policy: RedirectPolicy,
    pub accepted_content_types: &'static [&'static str],
}
```

### 10.3 Mandatory policy

Before sending:

1. Require HTTPS unless the engine explicitly permits HTTP.
2. Match the host against a compile-time allow-list.
3. Prevent user input from controlling the host.
4. Reject embedded credentials in the URL.
5. Build cookies through a typed cookie builder.
6. Default redirects to manual.
7. Revalidate each redirect host.
8. Strip cookies and sensitive headers on cross-host redirects.
9. Enforce maximum redirect count.
10. Enforce maximum body size while streaming.
11. Validate response content type.
12. Run challenge detection before parsing.
13. Abort when the engine deadline expires.
14. Never log cookies or sensitive headers.

### 10.4 Compression

Workers Fetch supports explicit `Accept-Encoding`, including gzip and Brotli. Let the Worker runtime handle network decompression rather than shipping Rust compression implementations in the initial bundle.

### 10.5 Experimental socket transport

Implement only as a separate compile feature:

```text
socket-http-lab
```

Purpose:

* Confirm Hyper `conn` operation over Workers Socket
* Compare HTTP/1.1 behaviour against Fetch
* Test a non-Cloudflare endpoint
* Measure bundle size and memory
* Determine whether header serialization changes provider responses

Cloudflare supports Hyper’s lower-level `conn` interface over Workers Socket, but recommends Fetch for HTTP traffic on ports 80 and 443. Outbound TCP to Cloudflare addresses is blocked.

This transport must not be used for production until it demonstrates a material compatibility benefit.

## 11. Multi-stage engine execution

### DuckDuckGo advanced engine

Current SearXNG behaviour:

```text
Fetch search HTML
    ↓
Extract generated preload/JSON URL
    ↓
Fetch JSON results
    ↓
Extract next continuation URL
    ↓
Cache continuation
```

The generated endpoint depends on query and User-Agent, and SearXNG caches continuation information by query and page.

### WolframAlpha

```text
Read cached code
    │
    ├── valid → query result endpoint
    │
    └── missing
          ↓
       refresh code through ProviderCoordinator
          ↓
       store expiry
          ↓
       query result endpoint
```

### Brave rich results

```text
Fetch public HTML
    ↓
Detect challenge or no-results page
    ↓
Stream HTML selectors
    ↓
Locate bounded embedded data where needed
    ↓
Parse provider-specific structure
```

### Engine limits

```text
Maximum steps per engine:       3
Maximum redirects per step:     2
Default engine deadline:        2,500 ms
HTML engine deadline:           3,500 ms
Default total search deadline:  5,000 ms
Maximum accepted deadline:      8,000 ms
```

## 12. Connection budgeting

Workers allow six simultaneous outbound connections waiting for initial response headers. Cache, KV, service and socket operations may also consume slots.

Planner defaults:

```text
Default engines:              4
Hard maximum engines:         5
Reserved control slot:        1
Maximum total subrequests:   10
Immediate retries:            0
Conditional network retry:    1
```

Execution:

```text
1. Authenticate and validate.
2. Read provider snapshot.
3. Check Cache API.
4. Select at most four default engines.
5. Launch one first-stage request per engine.
6. As headers arrive, released slots may be used by multi-stage engines.
7. Cancel remaining work at total deadline.
8. Return partial results when at least one engine succeeded.
```

## 13. Parser architecture

### JSON

Use typed Serde models where the provider schema is understood.

```rust
#[derive(Deserialize)]
#[serde(tag = "status", content = "data")]
enum ProviderResponse {
    Success { result: ProviderResult },
    Error { code: i32, message: Vec<String> },
}
```

This follows Websurfx’s strongest pattern while keeping parsing separate from transport.

### XML and Atom

Use `quick-xml` for:

* arXiv
* RSS
* Atom
* OpenSearch XML

The arXiv result model should preserve:

* Title
* Abstract
* Authors
* Canonical URL
* PDF URL
* DOI
* Journal
* Categories
* Comments
* Publication date

### Simple HTML

Adapt Websurfx’s configurable selector model:

```rust
pub struct SelectorResultSpec {
    pub no_results: Option<&'static str>,
    pub item: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    pub description: Option<&'static str>,
    pub thumbnail: Option<&'static str>,
}
```

Candidates:

* Wikipedia
* DuckDuckGo HTML
* Startpage
* Simple Brave result cards

### Streaming HTML

Use `lol-html` for:

* Early challenge detection
* Large pages
* Result containers identifiable by CSS selectors
* Bounded text and attribute extraction
* Embedded script discovery

`lol-html` is a low-buffering streaming parser with a CSS-selector API and is explicitly designed for memory-constrained environments.

### DOM parser gate

Websurfx uses `scraper` for full-document CSS traversal. The crate may compile to Wasm, but Cloudflare’s supported-crate page does not guarantee it.

Phase 0 must measure:

```text
Does it build for wasm32-unknown-unknown?
Compressed bundle increase
Peak memory on a 2 MiB page
Parser correctness on Google/Brave fixtures
CPU cost versus lol-html
```

Include it only if it solves complex traversal that would otherwise produce brittle streaming state machines.

### Embedded JavaScript data

Do not include a JavaScript runtime.

```rust
pub fn extract_embedded_value(
    body: &[u8],
    start_marker: &[u8],
    end_marker: &[u8],
    max_bytes: usize,
    max_depth: usize,
) -> Result<Vec<u8>, ParseError>;
```

Requirements:

* No code execution
* Maximum script length
* Maximum nesting depth
* Maximum string length
* No external script loading
* No unbounded regular expressions
* Provider-specific normalization only

## 14. Challenge and error classification

Model:

```rust
pub enum EngineFailure {
    Timeout,
    Network,
    RedirectRejected,
    AccessDenied,
    RateLimited {
        retry_after_seconds: Option<u64>,
    },
    Challenge {
        kind: ChallengeKind,
    },
    InvalidContentType,
    ResponseTooLarge,
    ParseFailure {
        parser_version: u16,
    },
    EmptyResultSet,
    Upstream {
        status: u16,
    },
    StateUnavailable,
}
```

SearXNG distinguishes ordinary upstream failures from Cloudflare challenges, firewall denials, CAPTCHA, access denial and rate limiting.

Google-specific detection should cover:

* `/sorry/` paths
* Challenge redirects
* Unexpected HTTP 302 responses
* Short challenge bodies
* Known CAPTCHA markers

No challenge response may be retried immediately.

## 15. Provider state and circuit breakers

Suggested suspension policy:

| Failure                 |           Initial cooldown |
| ----------------------- | -------------------------: |
| Network error           |                 10 seconds |
| HTTP 500–599            |                 30 seconds |
| HTTP 429                | `Retry-After` or 5 minutes |
| Access denied           |                 30 minutes |
| CAPTCHA                 |                   12 hours |
| Repeated CAPTCHA        |               Up to 7 days |
| Repeated parser failure |     Disable parser version |

State flow:

```text
Search request
    ↓
read in-memory provider snapshot
    ↓
plan engines
    ↓
execute
    ↓
send state mutations to per-provider Durable Object
    ↓
DO atomically updates provider record
    ↓
publish compact snapshot to KV
    ↓
Workers periodically refresh snapshot
```

Do not synchronously contact a Durable Object for every engine before every search. The snapshot may be slightly stale; exact mutation remains coordinated.

Durable Objects are single-threaded per object and have their own outgoing connection and throughput constraints, so use one named object per provider rather than one global metasearch singleton.

## 16. Cache specification

### Aggregate search cache

```text
search:{api_version}:{cache_hash}
```

Hash inputs:

```text
normalized query
sorted engine IDs
categories
page or cursor hash
limit
locale
country
safe-search
time range
engine registry version
parser versions
ranking version
```

### Per-engine normalized cache

```text
engine:{engine_id}:{parser_version}:{query_hash}
```

Store normalized engine output, not raw HTML.

### Negative cache

Cache briefly:

* Rate limits
* Challenges
* Access denial
* Known provider outage
* Parser incompatibility

Do not negative-cache a legitimate empty result as an infrastructure failure.

### TTLs

| Type                       |                        TTL |
| -------------------------- | -------------------------: |
| General web                |                2–5 minutes |
| News                       |              30–90 seconds |
| Academic                   |              30–60 minutes |
| Reference                  |                 30 minutes |
| Calculation                |               5–15 minutes |
| Partial aggregate response |              15–30 seconds |
| Challenge marker           | Based on provider cooldown |

### Cache writes

Use `ctx.waitUntil()` after constructing the response.

Cloudflare’s Cache API honours `Cache-Control`, but does not support `stale-while-revalidate` or `stale-if-error`. It also refuses responses carrying `Set-Cookie` unless the header is removed or made private.

## 17. Pagination

Support two forms.

### Numeric page

For deterministic providers:

* arXiv
* Wikipedia
* Brave
* Google
* Qwant
* Startpage

### Signed opaque cursor

For generated continuations:

* DuckDuckGo advanced
* Future token-based providers

Payload:

```json
{
  "version": 1,
  "query_hash": "sha256...",
  "expires_at": 1780000000,
  "continuations": {
    "duckduckgo-web": {
      "url": "https://links.duckduckgo.com/..."
    }
  }
}
```

Encode:

```text
base64url(payload) + "." + base64url(HMAC-SHA256(payload))
```

Validation:

1. Verify HMAC.
2. Verify expiry.
3. Verify query hash.
4. Verify engine ID.
5. Revalidate continuation hostname.
6. Reject oversized cursors.
7. Reject unsupported cursor versions.

## 18. Query normalization

Allowed:

* Trim whitespace
* Collapse repeated whitespace
* Unicode NFC normalization
* Validate maximum length
* Reject control characters
* Preserve quoted expressions
* Preserve provider operators

Do not:

* Lowercase the complete query
* Remove punctuation broadly
* Translate
* Correct spelling automatically
* Rewrite provider-specific syntax
* Expand synonyms

Limits:

```text
Minimum query length:       1 character
Maximum query length:     499 characters
Maximum selected engines:   5
Maximum categories:         3
Maximum cursor size:        8 KiB
```

The 499-character default accommodates DuckDuckGo’s current limit.

## 19. Result model

```rust
pub struct SearchResult {
    pub url: String,
    pub canonical_url: String,

    pub title: String,
    pub content: String,

    pub published_at: Option<OffsetDateTime>,
    pub thumbnail: Option<String>,

    pub category: SearchCategory,
    pub metadata: ResultMetadata,

    pub engines: Vec<String>,
    pub positions: BTreeMap<String, u32>,

    pub score: f64,
}
```

```rust
pub enum ResultMetadata {
    Web,

    News {
        publisher: Option<String>,
    },

    Academic {
        authors: Vec<String>,
        doi: Option<String>,
        journal: Option<String>,
        pdf_url: Option<String>,
        tags: Vec<String>,
    },

    Calculation {
        attributes: Vec<Attribute>,
    },

    Reference {
        source: Option<String>,
    },
}
```

## 20. URL canonicalization

Rules:

1. Parse with the `url` crate.
2. Allow only HTTP and HTTPS result URLs.
3. Lowercase hostname.
4. Remove fragments.
5. Remove default ports.
6. Normalize empty path to `/`.
7. Remove known tracking parameters.
8. Sort retained parameters.
9. Decode known provider redirect wrappers.
10. Normalize internationalized hostnames consistently.
11. Preserve parameters that change page content.
12. Do not automatically merge HTTP and HTTPS.

Initial tracking parameters:

```text
utm_*
gclid
fbclid
mc_cid
mc_eid
ref_src
```

Provider redirect decoding belongs in the provider module, not the global canonicalizer.

## 21. Deduplication

### MVP

Use exact canonical URL equality.

When merging:

* Preserve every engine
* Preserve every provider position
* Select the strongest non-empty title
* Select the strongest non-empty description
* Merge compatible metadata
* Record conflicting fields for debugging

### Post-MVP probable duplicate matching

Require:

* Same registrable domain
* Equivalent normalized path
* Strong title similarity
* Same category
* No conflicting publication dates
* No contradictory specialist metadata

Do not merge solely because titles are similar.

## 22. Ranking

Support two versioned ranking strategies.

### `rrf-v1`: default

```text
score =
    Σ engine_weight / (60 + provider_position)
    + 0.10 × ln(1 + number_of_engines)
```

Benefits:

* Deterministic
* Cheap
* Stable across providers
* Does not assume provider-native scores are comparable

### `searx-compat-v1`

Reproduce the documented SearXNG-style contribution:

```text
weight =
    product(engine weights)
    × number of contributing positions

score =
    Σ weight / provider_position
```

SearXNG’s current implementation multiplies contributing engine weights, scales by the number of positions and adds weight divided by each result position.

Do not use Websurfx’s Rayon-based TF-IDF ranker in the MVP. It adds CPU and native parallelism while ignoring the upstream providers’ ranking expertise.

## 23. Initial engine matrix

| Engine              | Endpoint type         | Initial maturity |              Default |
| ------------------- | --------------------- | ---------------: | -------------------: |
| arXiv               | Open Atom             |           Stable |    Academic searches |
| Wikipedia           | Public HTML           |           Stable |   Reference searches |
| DuckDuckGo HTML     | Public HTML           |             Beta |          General web |
| Brave web           | Public HTML           |             Beta |          General web |
| Qwant               | Public frontend JSON  |             Beta |          General web |
| DuckDuckGo advanced | HTML bootstrap + JSON |             Beta |   Disabled initially |
| WolframAlpha        | Token + JSON          |             Beta | Calculation searches |
| Brave news          | Public HTML           |             Beta |        News searches |
| Startpage           | Public HTML           |     Experimental |             Disabled |
| Google              | Public HTML           |     Experimental |             Disabled |

Default general search:

```text
DuckDuckGo HTML
+ Brave web
+ Qwant
+ Wikipedia
```

Google must never be required for a successful default search.

Every engine must be tested from actual Cloudflare production egress. A parser working from a local machine does not establish that the provider will accept Cloudflare Worker requests.

## 24. Engine manifest

```yaml
id: brave-web
display_name: Brave Search

source:
  kind: public_html
  allowed_hosts:
    - search.brave.com

capabilities:
  categories:
    - web
  paging: offset
  locale: true
  country: true
  safe_search: true
  time_range: true

execution:
  timeout_ms: 3500
  max_body_bytes: 2097152
  max_steps: 1
  max_redirects: 2

parser:
  version: 1
  fixtures:
    - normal.html
    - no-results.html
    - changed-layout.html
    - access-denied.html
    - challenge.html

state:
  policy: snapshot

cache:
  ttl_seconds: 180
  negative_ttl_seconds: 30

maturity: beta
default_enabled: true
weight: 1.0

provenance:
  implementation_mode: clean_room
  searxng_path: searx/engines/brave.py
  websurfx_path: src/engines/brave.rs
  searxng_commit: "<sha>"
  websurfx_commit: "<sha>"
```

## 25. API contract

API Shield requires OpenAPI 3.0.

### Search

```http
GET /v1/search
```

Parameters:

```text
q              required string
engines        optional comma-separated IDs
categories     optional comma-separated categories
page           optional integer
cursor         optional opaque cursor
limit          default 10, maximum 20
locale         optional BCP 47 locale
country        optional ISO 3166-1 alpha-2
safe_search    off | moderate | strict
time_range     day | week | month | year
ranking        rrf-v1 | searx-compat-v1
timeout_ms     maximum 8000
```

`page` and `cursor` are mutually exclusive.

### Structured search

```http
POST /v1/search
Content-Type: application/json
```

```json
{
  "query": "cloudflare rust workers",
  "engines": [
    "brave-web",
    "duckduckgo-html",
    "wikipedia-en"
  ],
  "categories": ["web"],
  "limit": 10,
  "locale": "en-CA",
  "country": "CA",
  "safe_search": "moderate",
  "time_range": "month",
  "ranking": "rrf-v1",
  "timeout_ms": 5000
}
```

### Response

```json
{
  "request_id": "01J...",
  "query": {
    "original": "cloudflare rust workers",
    "normalized": "cloudflare rust workers"
  },
  "results": [
    {
      "url": "https://example.com/page",
      "canonical_url": "https://example.com/page",
      "title": "Example result",
      "content": "Result description",
      "published_at": null,
      "thumbnail": null,
      "category": "web",
      "metadata": {
        "type": "web"
      },
      "engines": [
        "brave-web",
        "duckduckgo-html"
      ],
      "positions": {
        "brave-web": 2,
        "duckduckgo-html": 4
      },
      "score": 0.0358
    }
  ],
  "suggestions": [],
  "correction": null,
  "next_cursor": null,
  "engines": {
    "requested": [
      "brave-web",
      "duckduckgo-html",
      "wikipedia-en"
    ],
    "successful": [
      {
        "id": "brave-web",
        "duration_ms": 184,
        "result_count": 10,
        "cache": "miss"
      }
    ],
    "failed": []
  },
  "partial": false,
  "cache": {
    "status": "miss",
    "ttl_seconds": 180
  },
  "ranking_version": "rrf-v1",
  "duration_ms": 231
}
```

### Stable errors

```text
INVALID_REQUEST
AUTHENTICATION_REQUIRED
RATE_LIMITED
UNKNOWN_ENGINE
ENGINE_DISABLED
UNSUPPORTED_CAPABILITY
ENGINE_TIMEOUT
ENGINE_RATE_LIMITED
ENGINE_CHALLENGED
ENGINE_ACCESS_DENIED
ENGINE_RESPONSE_TOO_LARGE
ENGINE_INVALID_CONTENT_TYPE
ENGINE_PARSE_FAILED
NO_ENGINE_SUCCEEDED
INVALID_CURSOR
INTERNAL_ERROR
```

Use RFC 9457-style problem documents.

### SearXNG compatibility

```http
GET /search?q=...&format=json
```

Support:

```text
q
engines
categories
language
pageno
safesearch
time_range
format=json
```

Do not support HTML, RSS or CSV in the MVP.

## 26. Web Bot Auth

Cloudflare Web Bot Auth provides cryptographic identity for automated HTTP traffic through Ed25519 HTTP message signatures. A bot publishes a signed key directory, registers that directory and signs outbound requests.

Implement optionally:

```text
/.well-known/http-message-signatures-directory
/signature-agent-card
```

```rust
pub enum BotAuthPolicy {
    Disabled,
    Opportunistic,
    Required,
}
```

Web Bot Auth is useful only when the destination or its Cloudflare configuration recognises the signature. It does not solve CAPTCHA, Google anti-automation, provider rate limits or non-Cloudflare filtering.

Cloudflare’s example repository is Apache-2.0 but unaudited, and its sample-generated keys are explicitly unsuitable for production.

Production keys:

* Generate offline
* Store private key in Secrets or Secrets Store
* Publish only the public JWKS
* Rotate deliberately
* Use short signature expirations
* Never save private keys in KV

## 27. Observability

Use three layers.

### Workers Logs

Structured events:

```json
{
  "event": "search.completed",
  "request_id": "01J...",
  "query_hash": "sha256...",
  "requested_engines": 4,
  "successful_engines": 3,
  "failed_engines": 1,
  "result_count": 22,
  "cache": "miss",
  "duration_ms": 231
}
```

Workers Logs supports sampling and dashboard analysis.

### Workers traces

Create spans around I/O:

```text
search
├── cache.lookup
├── engine.brave.fetch
├── engine.brave.parse
├── engine.ddg.fetch
├── engine.ddg.parse
├── fusion
└── cache.write
```

Cloudflare notes that Rust spans without I/O may show identical start and end times, so record CPU durations explicitly when useful.

### Analytics Engine

Dimensions:

```text
engine_id
category
outcome
failure_kind
parser_version
cache_status
transport
colo
```

Measures:

```text
duration_ms
header_wait_ms
body_read_ms
parse_ms
response_bytes
result_count
steps
redirect_count
```

Analytics Engine writes are non-blocking.

Do not log raw query text by default.

### Wasm coredumps

Use Cloudflare’s Wasm coredump tooling in development and staging. It can capture Rust traps and store reports in R2 or send them to Sentry.

Because coredumps may contain memory containing queries or secrets, production use requires:

* Restricted R2 access
* Short retention
* Encryption
* Sampling
* Incident-only enablement

## 28. Scheduled provider canaries

Use Cron Triggers for low-frequency provider checks. Cron invokes a Worker’s `scheduled()` handler and runs in UTC.

Canaries:

```text
Every 15 minutes:
  one stable general engine

Hourly:
  all stable and beta engines

Daily:
  experimental engines
  parser fixture refresh alerts
  provider trait refresh
```

Use fixed benign queries.

Canaries must:

* Never retry CAPTCHA
* Never generate meaningful traffic volume
* Record schema and selector changes
* Update provider-state records
* Automatically downgrade broken engines
* Not block deployment CI

Saffron is unnecessary unless users can define arbitrary schedules. Cloudflare already runs Cron Triggers; Saffron is only a useful reference for parsing and validating cron syntax.

## 29. Security boundaries

### SSRF prevention

Clients cannot supply:

* Engine endpoints
* Arbitrary URLs
* Proxy addresses
* Outbound headers
* Cookies
* Redirect destinations
* Engine manifests

Each engine has a compile-time hostname allow-list.

### Result sanitisation

* Accept only HTTP and HTTPS URLs
* Strip control characters
* Bound title and description length
* Reject `javascript:` and `data:` URLs
* Do not return raw upstream HTML
* Do not proxy thumbnails in the MVP
* Do not preserve upstream cookies

### Identity

Use a stable, transparent User-Agent:

```http
User-Agent: NurauSearchBot/0.1 (+bot-information URL)
```

Do not implement:

* Random browser impersonation
* TLS fingerprint manipulation
* CAPTCHA bypass
* Source-IP rotation
* Proxy rotation
* Anti-detection delays
* Human-behaviour simulation

Websurfx’s random-User-Agent approach should not become the default Worker policy.

### Legal and provider policy

The fact that an endpoint is publicly reachable does not itself grant unrestricted automated use. Each engine manifest must document:

* Provider terms reviewed
* Applicable robots directives
* Rate limits
* Required attribution
* Permitted categories
* Operational owner
* Disable procedure

## 30. Licensing

SearXNG and Websurfx are both AGPL-licensed.

Choose one repository-wide mode.

### AGPL-derived implementation

Use when translating or adapting implementation code.

Requirements:

* License the covered Worker code under AGPL
* Preserve notices
* Mark modifications
* Publish corresponding source
* Publish build and deployment scripts
* Offer source access to remote users
* Record source path and commit per adapter

### Independent implementation

Use when a different licence is required.

Requirements:

* Use SearXNG and Websurfx to identify behaviour
* Verify behaviour independently
* Write original Rust control flow and types
* Avoid line-by-line translation
* Generate independent fixtures
* Record reference provenance
* Review each adapter independently

The fastest implementation path is AGPL-derived. The cleanest proprietary path is independent reimplementation. This is an engineering assessment, not legal advice.

## 31. Testing

### Pure Rust tests

* Query normalization
* URL canonicalization
* Deduplication
* Ranking
* Cursor signing
* JSON parsing
* XML parsing
* HTML parsing
* Embedded-data extraction
* Challenge detection
* Cache-key generation

### Wasm build matrix

```text
default
reqwest-facade
dom-parser
web-bot-auth
socket-http-lab
wasm-coredump
```

Every feature combination must either compile for `wasm32-unknown-unknown` or be explicitly native-only.

### Provider fixtures

Each engine requires:

```text
normal response
empty response
changed layout or schema
access denied
rate limited
challenge or CAPTCHA
truncated response
oversized response
wrong content type
redirect response
```

### Differential tests

Compare the same fixture against:

* SearXNG
* Websurfx, where it has the same provider
* The Rust Worker parser

Compare:

* URLs
* Titles
* Descriptions
* Publication dates
* Suggestions
* Specialist metadata
* Error classification

Exact ranking equivalence is required only in `searx-compat-v1`.

### Worker integration tests

Run through Wrangler/workerd:

* Routes
* Authentication
* Cache API
* KV
* Durable Objects
* Rate limiting
* Fetch mocks
* AbortController
* Partial failures
* Cursor validation
* Response schemas
* Scheduled canaries

### Fuzzing

Fuzz natively:

* URL canonicalizer
* Cursor decoder
* Embedded-data parser
* HTML extractors
* XML models
* JSON boundaries
* Challenge detector

No malformed response may panic the Worker.

## 32. CI and deployment

```text
1. cargo fmt --check
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. cargo test --workspace
4. validate engine manifests
5. validate OpenAPI 3.0
6. validate examples against schemas
7. build wasm32-unknown-unknown
8. build each feature combination
9. run wasm-opt
10. enforce compressed bundle budget
11. run workerd integration tests
12. run fixture differential tests
13. deploy preview Worker
14. run preview smoke tests
15. deploy canary percentage
16. evaluate errors and latency
17. promote or roll back
```

Internal budgets:

```text
Compressed Worker:             under 6 MiB
Peak request memory:           under 64 MiB
Warm orchestration CPU:        under 25 ms
Fusion of 100 results:         under 5 ms
Canonicalization of 100 URLs:  under 2 ms
Default total response:        under 5 seconds
Maximum upstream body:         2 MiB
Maximum normalized results:    250
```

Platform limits remain higher, including 128 MB isolate memory and up to 10 MB compressed Worker size on paid plans.

## 33. Milestones

### Milestone 0 — feasibility gates

Deliver:

* Licence decision
* `workers-rs` skeleton
* `reqwest` Wasm compile and size benchmark
* `scraper` Wasm compile and size benchmark
* `lol-html` parser benchmark
* Hyper-over-Socket proof of concept
* Rustls-over-Socket research result
* Privacy Proxy feasibility memo
* OpenAPI skeleton

Acceptance:

* No unproven transport is in the production path
* Every dependency has a Wasm build result
* Search authentication is compatible with Cache API
* Licence mode is resolved

### Milestone 1 — Worker foundation

Deliver:

* Router
* Authentication
* Rate limiting
* OpenAPI models
* Health routes
* Workers Logs
* Analytics Engine
* CI Wasm build

### Milestone 2 — transport and state

Deliver:

* Restricted Fetch client
* Abort deadlines
* Manual redirects
* Cookie builder
* Body limits
* Cache API
* KV snapshot
* ProviderCoordinator Durable Object
* Challenge classifier

### Milestone 3 — core metasearch

Deliver:

* Engine trait
* Engine registry
* Planner
* Query normalization
* Canonicalization
* Exact deduplication
* `rrf-v1`
* `searx-compat-v1`
* Partial success model

### Milestone 4 — low-risk engines

Implement:

1. arXiv
2. Wikipedia
3. DuckDuckGo HTML

Acceptance per engine:

* Manifest
* Fixtures
* Request tests
* Parser tests
* Canary
* Cache policy
* Error classification
* Disable switch

### Milestone 5 — default general search

Implement:

1. Brave web
2. Qwant public frontend JSON
3. Aggregate four-engine search

Acceptance:

* At least two independent general engines succeed
* One blocked engine does not fail the search
* Duplicate merging works
* Ranking is deterministic
* Cache hit and miss schemas match

### Milestone 6 — stateful engines

Implement:

1. DuckDuckGo advanced
2. WolframAlpha
3. Brave news

Acceptance:

* Continuation cursor works
* Token refresh is single-flight
* Expired state recovers safely
* Provider cooldown is shared
* No state service becomes a global bottleneck

### Milestone 7 — production API

Deliver:

* GET and POST search
* Engine catalogue
* Single-engine route
* SearXNG JSON compatibility
* API Shield schema
* mTLS/JWT/API-key production auth
* Operational dashboards

### Milestone 8 — experimental engines

Implement behind disabled flags:

```text
ENABLE_STARTPAGE=false
ENABLE_GOOGLE=false
ENABLE_SOCKET_HTTP=false
ENABLE_PRIVACY_PROXY_LAB=false
```

Google acceptance:

* CAPTCHA detection
* Immediate cooldown
* No immediate retry
* Layout failures are safe
* Never required for default search

### Milestone 9 — transparent bot identity

Deliver:

* Web Bot Auth key workflow
* Signed key directory
* Agent card
* Optional request signing
* Verification tests

### Milestone 10 — hardening

Deliver:

* Fuzzing
* Load tests
* Memory profiling
* Parser mutation tests
* Gradual deployment
* Rollback runbook
* Licence/source publication process
* Provider policy registry

## 34. Complete supplied-resource disposition

### Integrate in the MVP

| Resource              | Decision                                    |
| --------------------- | ------------------------------------------- |
| `workers-rs`          | Core runtime                                |
| Workers Fetch         | Production HTTP transport                   |
| Cache API             | PoP-local parsed-result and aggregate cache |
| KV                    | Slow-changing global snapshots              |
| Durable Objects       | Exact per-provider coordination             |
| `lol-html`            | Streaming HTML parser                       |
| `serde_json`          | JSON parsers                                |
| `quick-xml`           | Atom and XML parsers                        |
| `web-bot-auth`        | Optional signed bot identity                |
| `wasm-coredump`       | Development/staging crash diagnostics       |
| Cloudflare API Shield | OpenAPI validation                          |
| mTLS                  | Search API authentication                   |
| Analytics Engine      | Provider telemetry                          |
| Workers Logs/traces   | Operational observability                   |

### Use as implementation or design references

| Resource                | Decision                                     |
| ----------------------- | -------------------------------------------- |
| SearXNG                 | Primary engine-behaviour reference           |
| Websurfx                | Rust adapter and parser reference            |
| Freighter               | Modular provider-trait reference             |
| networkquality-rs       | Transport abstraction reference              |
| Chaussette              | Privacy Proxy and CONNECT-client reference   |
| `cf-reqwest`            | Wasm HTTP facade/reference                   |
| Rustls                  | Experimental TLS-over-Socket research        |
| Hyper `conn`            | Experimental Socket HTTP client              |
| Wirefilter              | Future operator policy language              |
| Saffron                 | Future user-defined schedule parser          |
| Wildcard                | Optional hostname-pattern matching           |
| `cloudflare-rs`         | Optional deployment/control-plane automation |
| `svg-hush`              | Future proxied SVG thumbnail sanitisation    |
| `entropy-map`           | Future large immutable trait tables          |
| `cardinality-estimator` | Future approximate telemetry aggregation     |
| LazyHTML                | Parser-design reference only                 |

### Exclude from the Worker runtime

| Resource                         | Reason                                                   |
| -------------------------------- | -------------------------------------------------------- |
| Pingora                          | Native Linux proxy framework                             |
| Quiche                           | Requires UDP packet I/O and native QUIC runtime          |
| Quiche Mallard                   | Same transport mismatch                                  |
| BoringTun                        | Userspace WireGuard requires tunnel/network-device model |
| Boring                           | Native BoringSSL bindings                                |
| Chaussette binary/library        | Native Tokio SOCKS listener and proxy client             |
| Foundations                      | Native service runtime, allocator and sandbox facilities |
| mmap-sync                        | Memory-mapped files and inter-process coordination       |
| trie-hard                        | Unnecessary optimisation for the small registry          |
| sliceslice-rs                    | x86/AVX2-specific optimisation                           |
| shellflip                        | Native process forking and graceful restart              |
| ecdysis                          | Native process/socket inheritance                        |
| wrangler-legacy                  | Deprecated                                               |
| rustwasm-worker-template         | Superseded by current workers-rs templates               |
| ODoH repositories                | DNS privacy protocol, unrelated to metasearch transport  |
| Privacy Gateway client library   | Mobile OHTTP demonstration, not a Worker proxy           |
| cfnts                            | Network time-security implementation                     |
| Daphne                           | Distributed aggregation protocol                         |
| moq-rs                           | Media over QUIC                                          |
| recapn                           | Cap’n Proto implementation, no MVP need                  |
| psi_exporter                     | Linux kernel pressure telemetry                          |
| chrome-devtools-rs               | Repository explicitly warns not to use it                |
| DKIM/DMARC                       | Email authentication                                     |
| Azul/Plexi                       | Transparency and key-auditing systems                    |
| workers-wonnx                    | WebGPU model example                                     |
| ssh-log-cli                      | SSH logging utility                                      |
| matched-data-cli                 | Firewall matched-data CLI                                |
| nel-rs                           | Network Error Logging report utilities                   |
| aloha-rs                         | No demonstrated metasearch requirement                   |
| pfp-tools                        | Programmable Flow Protection tooling                     |
| BoringTun-related VPN components | No TUN, UDP or tunnel device in Workers                  |
| moq/quic media projects          | No metasearch role                                       |
| native Prometheus forks          | Use Workers observability and Analytics Engine           |

Cloudflare’s Rust repository inventory confirms that these projects cover very different domains and runtime assumptions; repository language alone does not establish Worker compatibility.

## 35. MVP completion definition

The MVP is complete when a client can send:

```http
GET /v1/search?q=cloudflare+rust
Authorization: Bearer <key>
```

and receive:

* Authenticated access
* OpenAPI-validated input
* A Cache API lookup
* Concurrent open-endpoint searches
* At least two general-web providers
* Bounded HTML, JSON and XML responses
* Challenge classification
* Normalized results
* Canonical URLs
* Duplicate merging
* Deterministic fusion
* Provider execution metadata
* Partial-failure tolerance
* Signed continuation cursors where required
* Structured logs and analytics
* No paid search API dependency
* No browser
* No container
* No external proxy
* No native server

The production request path is:

```text
Cloudflare edge
    ↓
WAF + API Shield + mTLS/JWT/API key
    ↓
Rust Worker
    ↓
Cache API
    ↓
Engine planner
    ↓
Workers Fetch
    ↓
Open search endpoints
    ↓
Rust parsers
    ↓
Normalization, deduplication and ranking
    ↓
JSON response
```

The socket, Rustls, Chaussette and Privacy Proxy work remains an explicitly isolated research track. None of it blocks the initial useful metasearch API.
