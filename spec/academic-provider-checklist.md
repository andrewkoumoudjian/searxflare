# Academic provider implementation checklist

Status: implementation in progress on `feat/academic-providers`.

## Scope

- [x] PubMed ESearch and ESummary adapter
- [x] Semantic Scholar relevance-search adapter
- [x] Crossref works-search adapter
- [x] Fixed compile-time outbound hosts
- [x] Bounded paging, result counts, response bodies and request steps
- [x] Provider-specific metadata normalization
- [x] Normal, empty, changed-schema and rate-limit fixtures
- [x] Engine manifests
- [x] Unit request and parser tests
- [x] workerd catalogue and explicit three-provider fan-out test
- [x] README, limitations and source notes
- [ ] Exact-head formatting, Clippy, native tests and Wasm build
- [ ] OpenAPI and manifest validation
- [ ] Exact-head workerd integration suite
- [ ] Wrangler dry-run and bundle-size validation
- [ ] Cloudflare preview deployment and live provider smoke checks
- [ ] PR evidence sync and merge

## Deliberate exclusions

- OpenAlex, pending provider-scoped secret plumbing for its required API key
- Semantic Scholar API-key support
- Crossref operator `mailto` until a service contact address is configured
- PubMed abstract EFetch, which would add a third upstream request
- Embedding or cross-encoder reranking in the request Worker
- Default enabling of the new providers

## Rollback

The engines are default-disabled. Immediate operational rollback is to stop selecting `pubmed`, `semantic-scholar` and `crossref`. Repository rollback is a revert of the slice; deployment rollback is promotion of the previous Worker version.
