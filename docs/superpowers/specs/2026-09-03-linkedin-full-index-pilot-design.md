# LinkedIn Full-Index Pilot Design

## Goal

Finish the validated LinkedIn indexing pilot that grew out of the `searxflare` UA-spoof experiment: recover the authoritative Apify actor into the repository, retain complete public SSR content, run seed-only crawls without wasting retries on deterministic `999` responses, normalize the existing enrichment corpus by canonical LinkedIn URL, and prove dense+sparse hybrid retrieval in a local Qdrant collection named `linkedin-full`.

This slice ends at a working pilot index and verified hybrid queries. Recursive BFS expansion and a public search API are deliberately deferred until the pilot corpus is correct and measurable.

## Boundaries

- Public LinkedIn SSR pages only.
- No Voyager API, LinkedIn cookies, authenticated sessions, browser automation, CAPTCHA solving, or residential proxies by default.
- The crawler remains a lean Apify actor built from `httpx`, BeautifulSoup/lxml, and simple async primitives.
- Qdrant and FastEmbed run locally and never inside the Apify actor.
- `searxflare` Worker search/crawl behavior is not changed by this slice.
- Existing structured fields are preserved losslessly with source provenance instead of flattened destructively.

## Verified starting state

- Repository: `andrewkoumoudjian/searxflare`, main at `ffec2c2` plus the local worktree setup commit.
- Apify actor: `andrewthecube/linkedin-crawler-actor` (`TmxkNceI631zStdDV`).
- Latest actor build: `0.0.17`, successful.
- Validation run `GRVGg52rbcQrhlkje`: 10 records, 5 public SSR successes and 5 deterministic `999` failures.
- Successful 0.0.17 records contain JSON-LD, visible text, and Markdown, but omit raw HTML and truncate text to 262,144 chars and Markdown to 1,048,576 chars.
- The 500-seed baseline timed out after one hour with 128 records because each `999` URL rotates through multiple UAs with exponential sleeps even though controlled experiments established that visibility is URL-level deterministic.
- The old local Qdrant container is gone and `linkedin-full` has not been proved end-to-end.

## Repository layout

The LinkedIn-specific pipeline lives under `tools/linkedin_index/` so it is versioned with the `searxflare` experiment that established the access behavior, without coupling Python indexing dependencies into the Rust Worker build.

```text
tools/linkedin_index/
├── pyproject.toml
├── actor/                     # source mirrored from authoritative Apify actor
├── src/linkedin_index/
│   ├── canonical.py           # LinkedIn URL canonicalization
│   ├── normalize.py           # CSV/XLSX structured-data merge + provenance
│   ├── corpus.py              # scraped + structured document assembly
│   ├── qdrant_index.py         # collection creation, embedding, upsert, hybrid query
│   └── pilot.py               # chunked Apify seed-only execution + result aggregation
└── tests/
```

## Actor behavior

The repository copy is derived from actor build `0.0.17`, but fixes the pilot-specific failure modes:

1. `parse_page` stores complete `html`, visible `text`, Markdown, JSON-LD, headings, metadata, and discovered canonical LinkedIn links. No content-length slicing is applied after a successful public SSR fetch.
2. HTTP `999` is terminal for that URL in the current run. It produces one failure record immediately because UA rotation has been experimentally shown not to change the outcome.
3. HTTP `429` and network errors remain transient and may retry with bounded exponential backoff.
4. `404` and other non-transient 4xx responses are terminal.
5. Every seed produces exactly one dataset record, successful or failed.
6. Pilot runs use `maxDepth=0`; BFS logic remains available but is not enabled until hybrid retrieval passes.

## Pilot execution

`pilot.py` accepts canonical seed URLs, deduplicates them, splits them into deterministic chunks, and invokes the authenticated Apify CLI for actor `TmxkNceI631zStdDV`. Each chunk uses datacenter `auto`, `maxDepth=0`, bounded concurrency, and one attempt per deterministic person-page response. The runner records run IDs and dataset IDs, downloads every dataset, and writes a manifest that can be resumed without re-running completed chunks.

This converts the one-hour monolithic 500-seed run into resumable, independently verifiable jobs while preserving exactly-once accounting at the manifest layer.

## Structured normalization

Every input row is retained as a source record. Canonical LinkedIn URLs are the primary merge key when available. Canonicalization:

- requires `linkedin.com`;
- converts HTTP to HTTPS;
- normalizes accepted locale hosts to `www.linkedin.com`;
- strips query strings and fragments;
- removes the trailing slash;
- preserves the slug bytes/encoding rather than decoding and re-encoding them.

Merged entities preserve all non-empty fields. When multiple sources disagree, the merged payload stores the values by source instead of silently choosing one. Common fields such as email, phone, name, title, company, location, industry, skills, education, and source IDs are additionally surfaced as convenience fields when they have a single unambiguous value.

## Search document

Each canonical URL becomes one Qdrant point. The searchable text is a deterministic composition of structured identity fields followed by public LinkedIn title, description, headings, visible text, and Markdown. The payload keeps `scraped` and `enrichment` objects separate, plus top-level filter fields (`url`, `entity_type`, `name`, `company`, `email`, `location`, `industry`, `fetched_at`, `source_files`).

Point IDs are deterministic UUIDv5 values derived from the canonical URL, making repeated ingestion idempotent.

## Qdrant and embeddings

Collection: `linkedin-full`.

- Dense model: `BAAI/bge-small-en-v1.5` through FastEmbed.
- Sparse model: `Qdrant/bm25` through FastEmbed.
- Named vectors: `dense` and `sparse`.
- Dense distance: cosine.
- Sparse vector configuration enables IDF where supported by the installed Qdrant client/server.
- Hybrid queries prefetch dense and sparse candidates and fuse them using Qdrant reciprocal-rank fusion.

Qdrant's current FastEmbed documentation supports local dense+sparse embeddings and RRF hybrid retrieval through the Python client. Model names are pinned in code/config so later FastEmbed defaults cannot silently change the index semantics.

## Error handling and observability

- Invalid LinkedIn URLs are rejected during normalization with a reason count; source rows are not deleted.
- A failed Apify chunk remains incomplete in the manifest and can be resumed.
- Dataset download or schema failures stop ingestion for that chunk instead of producing partial points silently.
- Qdrant collection configuration is checked before ingestion; incompatible vector configuration is an explicit error rather than an automatic destructive recreation.
- Every CLI stage emits machine-readable JSON summaries with counts for source rows, canonical entities, crawl statuses, points inserted, and query results.

## Verification

Automated tests cover canonicalization, conflict-preserving merges, full-content actor parsing, terminal `999` behavior, deterministic chunking/resume semantics, document assembly, deterministic point IDs, and hybrid Qdrant requests.

Runtime acceptance for this slice:

1. Actor source builds successfully on Apify.
2. A known-public real profile produces complete HTML/text/Markdown/JSON-LD.
3. A known `999` profile is attempted once and produces one failure record without UA-matrix delay.
4. The seed pilot can be resumed by chunks and produces complete accounting.
5. Normalized enrichment retains email/phone and arbitrary source-specific fields.
6. `linkedin-full` contains real points with dense+sparse vectors.
7. Representative semantic + lexical queries return sensible real records through RRF.

## Deferred work

- Recursive BFS beyond depth zero.
- Scheduled incremental recrawls.
- Public `/v1/search` LinkedIn endpoint or integration into the Rust Worker response path.
- Remote/hosted Qdrant.
- Authenticated LinkedIn access.
