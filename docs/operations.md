# Production operations

## Edge controls

The Worker applies `SEARCH_RATE_LIMITER` to every search route and hashes the bearer credential before using it as the limiter key. The SearXNG compatibility route now requires the same bearer authentication as `/v1/search`. The binding is deliberately a fast, per-location abuse control rather than an accounting system.

Import `spec/openapi.yaml` into API Shield Schema Validation 2.0 and activate it in `log` mode. Review Security Events for legitimate requests rejected by the schema, then change the default and operation-specific actions to `block`. Keep the specification at OpenAPI 3.0 because API Shield does not accept OpenAPI 3.1.

Use the `Authorization` header as the API Shield session identifier. Add mTLS or Cloudflare JWT validation when client certificate or identity-provider lifecycle is available; the Worker API key remains the application-level fallback.

## Required bindings and secrets

- `API_KEY_SHA256`: SHA-256 of the application bearer key.
- `CURSOR_SIGNING_KEY`: at least 32 random bytes represented as a secret string.
- `PROVIDER_COORDINATOR`: one Durable Object namespace sharded by provider or query coordination key.
- `SEARCH_RATE_LIMITER`: per-client search limiter.
- `ENGINE_STATE`: optional KV namespace for expiring provider continuation snapshots.
- `SEARCH_ANALYTICS`: optional Analytics Engine dataset.
- `CRAWL_DOCUMENTS`: R2 bucket containing one Markdown document per captured page.
- `CRAWL_STATE`: KV namespace for page freshness markers and the bounded crawl frontier.
- `CRAWL_SEARCH`: optional AI Search instance binding for the R2 corpus.

Provider credentials are isolated by binding name: `GITHUB_TOKEN`, `SEMANTIC_SCHOLAR_API_KEY`, `OPENALEX_API_KEY`, `WOLFRAM_APP_ID`, `EXA_API_KEY`, and `CROSSREF_MAILTO`.

## Crawl and AI Search limits

The post-response crawler fetches at most `CRAWL_MAX_PAGES_PER_QUERY` result pages, defaults to three, accepts HTTPS HTML only, caps each transformed body at 1 MiB and retained text at 256 KiB, and keeps at most 64 discovered links. Page freshness markers suppress repeat capture for 24 hours. The scheduled frontier drains four URLs every 30 minutes and stores at most twelve new frontier links per page.

The production `searxflare` AI Search instance targets the `searxflare-crawl-documents` R2 bucket with both vector and keyword indexes, `fusion_method=rrf`, query rewriting off, and model reranking off. The Worker requests at most five indexed chunks and applies its deterministic query-aware reranker after merging them with provider results. These settings bound per-query inference to one embedding plus hybrid retrieval; they do not create an unlimited generative-model path.

## Dashboard queries

The Analytics Engine blobs are, in order: engine ID, operation, outcome, failure kind, parser version, cache status, and colo. Doubles are duration, response bytes, parse duration, result count, and redirect count.

```sql
SELECT
  blob1 AS engine,
  blob3 AS outcome,
  blob4 AS failure_kind,
  COUNT(*) AS requests,
  quantileWeighted(double1, _sample_interval, 0.95) AS p95_duration_ms
FROM searxflare_search
WHERE timestamp > NOW() - INTERVAL '1' HOUR
GROUP BY engine, outcome, failure_kind
ORDER BY requests DESC;
```

```sql
SELECT
  blob1 AS engine,
  blob6 AS cache_status,
  COUNT(*) AS requests
FROM searxflare_search
WHERE timestamp > NOW() - INTERVAL '24' HOUR
GROUP BY engine, cache_status
ORDER BY engine, requests DESC;
```

Alert on readiness failures, sustained `NO_ENGINE_SUCCEEDED`, provider challenge/rate-limit growth, p95 duration above the five-second service objective, and bundle or memory guard failures.

## Durable Object and gradual rollout

Deploy `v1-provider-coordinator` as an isolated atomic deployment before using traffic splitting. Durable Object lifecycle migrations cannot be introduced through a gradual deployment. After the migration exists, upload later code-only versions with `wrangler versions upload` and split traffic with:

```bash
scripts/deploy_gradual.sh PREVIOUS_VERSION NEW_VERSION 5
scripts/deploy_gradual.sh PREVIOUS_VERSION NEW_VERSION 25
scripts/deploy_gradual.sh PREVIOUS_VERSION NEW_VERSION 100
```

Stop progression on elevated failures or latency. Restore 100 percent to the previous version when the change is code-only. A Durable Object lifecycle migration itself is not rolled back through traffic splitting.

## Hardening commands

```bash
cargo install cargo-fuzz
cargo fuzz run canonicalization -- -max_total_time=300
cargo fuzz run html_parser_mutation -- -max_total_time=300
cargo run -p metasearch-core --example memory_profile --release
k6 run tests/load/search.js
```

The DHAT run writes `dhat-heap.json` and fails when peak live bytes exceed the architecture’s 64 MiB request target.
