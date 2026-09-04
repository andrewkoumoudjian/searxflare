# LinkedIn Full-Index Pilot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a resumable Apify seed crawl, lossless enrichment normalization, and a real local `linkedin-full` Qdrant hybrid index.

**Architecture:** Keep the LinkedIn pipeline as isolated Python tooling under `tools/linkedin_index/`; mirror the authoritative Apify actor there, normalize structured data by canonical LinkedIn URL, assemble one idempotent document per URL, and embed/upsert dense+sparse vectors into local Qdrant. The existing Rust Worker is unchanged in this slice.

**Tech Stack:** Python 3.11+, pytest, httpx, BeautifulSoup/lxml, markdownify, Apify CLI/SDK, qdrant-client with FastEmbed, pandas/openpyxl for enrichment inputs, Docker Qdrant.

**Spec:** `docs/superpowers/specs/2026-09-03-linkedin-full-index-pilot-design.md`

## Global Constraints

- Public LinkedIn SSR only; no Voyager, cookies, authenticated sessions, browsers, CAPTCHA solving, or residential proxy by default.
- Actor crawling uses simple request/parser/async primitives; do not introduce Crawlee.
- Qdrant/FastEmbed stay outside the Apify actor.
- Collection name is exactly `linkedin-full`.
- Dense model is `BAAI/bge-small-en-v1.5`; sparse model is `Qdrant/bm25`; fusion is RRF.
- Pilot crawling is depth zero and resumable; recursive BFS is deferred.
- Production code is written test-first: every behavior change must have a failing test observed before implementation.

---

### Task 1: Python package and canonical entity normalization

**Files:**
- Create: `tools/linkedin_index/pyproject.toml`
- Create: `tools/linkedin_index/src/linkedin_index/__init__.py`
- Create: `tools/linkedin_index/src/linkedin_index/canonical.py`
- Create: `tools/linkedin_index/src/linkedin_index/normalize.py`
- Create: `tools/linkedin_index/tests/test_canonical.py`
- Create: `tools/linkedin_index/tests/test_normalize.py`

**Interfaces:**
- Produces `canonical_linkedin_url(value: str) -> str | None`.
- Produces `merge_records(rows: Iterable[SourceRecord]) -> dict[str, NormalizedEntity]` where each entity retains raw source fields and conflicts by source.

- [ ] Write tests proving host/query/fragment/trailing-slash normalization, invalid-host rejection, percent-encoded slug preservation, arbitrary-column retention, email/phone preservation, and conflicting values from two sources.
- [ ] Run `python -m pytest tools/linkedin_index/tests/test_canonical.py tools/linkedin_index/tests/test_normalize.py -q` and observe failures because the package does not exist.
- [ ] Add the package and minimal canonicalization/merge implementation.
- [ ] Run the same tests and make them pass.
- [ ] Commit as `feat(linkedin-index): normalize enrichment records`.

### Task 2: Recover and harden the Apify actor

**Files:**
- Create: `tools/linkedin_index/actor/.actor/actor.json`
- Create: `tools/linkedin_index/actor/.actor/input_schema.json`
- Create: `tools/linkedin_index/actor/my_actor/__init__.py`
- Create: `tools/linkedin_index/actor/my_actor/__main__.py`
- Create: `tools/linkedin_index/actor/my_actor/main.py`
- Create: `tools/linkedin_index/actor/Dockerfile`
- Create: `tools/linkedin_index/actor/requirements.txt`
- Create: `tools/linkedin_index/tests/test_actor.py`

**Interfaces:**
- `parse_page(url, html, ua_used, attempts)` returns complete HTML/text/Markdown/JSON-LD without slicing.
- `retry_decision(status: int | None, attempt: int) -> RetryDecision` treats 999/404 as terminal and 429/network failures as retryable.

- [ ] Write fixture tests with HTML larger than the old truncation thresholds and tests proving `999` is terminal while `429` is retryable.
- [ ] Run `python -m pytest tools/linkedin_index/tests/test_actor.py -q` and observe the expected failures.
- [ ] Mirror build 0.0.17 source into `actor/`, remove content slicing, include `html`, and factor retry classification so the tested behavior drives the crawl loop.
- [ ] Run actor tests to green and `python -m compileall -q tools/linkedin_index/actor/my_actor`.
- [ ] Commit as `fix(linkedin-index): retain full SSR and stop retrying 999`.

### Task 3: Resumable pilot runner

**Files:**
- Create: `tools/linkedin_index/src/linkedin_index/pilot.py`
- Create: `tools/linkedin_index/tests/test_pilot.py`

**Interfaces:**
- `chunk_seeds(urls: Iterable[str], size: int) -> list[list[str]]` is deterministic after canonical deduplication.
- `PilotManifest` records chunk hash, actor run ID, dataset ID, status, and local dataset path.
- CLI: `python -m linkedin_index.pilot --seeds <file> --out <dir> --chunk-size <n>`.

- [ ] Write tests proving stable chunking, duplicate removal, and that completed manifest chunks are skipped on resume while failed/incomplete chunks are eligible to run.
- [ ] Run `python -m pytest tools/linkedin_index/tests/test_pilot.py -q` and observe failures.
- [ ] Implement pure chunk/manifest logic, then the thin Apify CLI adapter using authenticated `apify actors call`/dataset export commands.
- [ ] Run pilot tests to green and exercise `--help` locally.
- [ ] Commit as `feat(linkedin-index): add resumable Apify pilot runner`.

### Task 4: Search-document assembly and Qdrant hybrid index

**Files:**
- Create: `tools/linkedin_index/src/linkedin_index/corpus.py`
- Create: `tools/linkedin_index/src/linkedin_index/qdrant_index.py`
- Create: `tools/linkedin_index/tests/test_corpus.py`
- Create: `tools/linkedin_index/tests/test_qdrant_index.py`

**Interfaces:**
- `build_document(scraped: dict, enrichment: NormalizedEntity | None) -> IndexDocument`.
- `point_id(url: str) -> str` returns UUIDv5 from canonical URL.
- `LinkedInIndex.ensure_collection()` validates/creates `linkedin-full` with named `dense` and `sparse` vectors.
- `LinkedInIndex.upsert(documents)` embeds with the pinned FastEmbed models and performs idempotent upserts.
- `LinkedInIndex.search(query, limit=10, filters=None)` issues dense+sparse prefetches fused by Qdrant RRF.

- [ ] Write tests for deterministic text/payload assembly, point IDs, vector names/model constants, incompatible collection detection, and the shape of the RRF hybrid query.
- [ ] Run the focused tests and observe failures.
- [ ] Implement document assembly and Qdrant/FastEmbed integration using the official client interfaces.
- [ ] Run focused tests to green.
- [ ] Commit as `feat(linkedin-index): add FastEmbed Qdrant hybrid index`.

### Task 5: Real runtime validation

**Files:**
- Modify: `tools/linkedin_index/README.md`
- Create/update: runtime artifacts under a git-ignored `tools/linkedin_index/.runs/` only.

**Interfaces:**
- Uses Tasks 1-4 without new public APIs.

- [ ] Pull the current Apify actor again and diff it against the vendored source to document intentional changes only.
- [ ] Push/build the actor and verify the new build succeeds; do not claim deployment until Apify reports terminal `SUCCEEDED`.
- [ ] Run one known-public control and one known-999 control; verify full-content fields and single-attempt 999 behavior from the returned dataset.
- [ ] Build a canonical seed set from available enrichment files, run resumable depth-zero chunks, and aggregate exact status/cost counts.
- [ ] Start a local Qdrant container, normalize available enrichment, ingest successful crawl records, and record collection point count/config.
- [ ] Run representative hybrid queries including `Montreal Manulife wealth advisor` and `food safety operations executive`; inspect real returned payloads.
- [ ] Run `python -m pytest tools/linkedin_index/tests -q`, `cargo test --locked --workspace`, and `npm test`.
- [ ] Commit validation docs as `docs(linkedin-index): record pilot validation`.
