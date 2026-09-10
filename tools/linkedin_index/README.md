# LinkedIn public index tooling

This package turns existing LinkedIn enrichment exports into canonical entities, crawls public LinkedIn SSR pages through the Apify actor, and builds a local dense+sparse Qdrant index. It does not use LinkedIn cookies, Voyager APIs, browser automation, or authenticated sessions.

## Pipeline

Normalize the source tree first:

```bash
PYTHONPATH=src .venv/bin/python -m linkedin_index.source_loader \
  --root /path/to/source-data \
  --out /path/to/runtime \
  --pilot-size 500
```

Run a bounded seed-only pilot when validating actor behavior:

```bash
PYTHONPATH=src .venv/bin/python -m linkedin_index.pilot \
  --seeds /path/to/runtime/pilot-seeds.json \
  --out /path/to/pilot-run \
  --chunk-size 20 \
  --concurrency 4
```

Continue as a durable breadth-first crawl. Existing paid datasets can be absorbed as depth zero, so restarting the crawler does not re-request those URLs:

```bash
PYTHONPATH=src .venv/bin/python -m linkedin_index.crawl \
  --seeds /path/to/runtime/pilot-seeds.json \
  --out /path/to/crawl \
  --bootstrap-dataset /path/to/pilot-run/chunk-000.json \
  --max-depth 1 \
  --include-root company \
  --include-root in \
  --max-nodes-per-depth 20 \
  --chunk-size 20 \
  --concurrency 4
```

`--max-nodes-per-depth` is a per-invocation safety bound. The crawler does not advance to the next depth while eligible nodes remain at the current depth, preserving a real BFS barrier and preventing a bounded canary from expanding transitively by accident.

Ingest successful crawl datasets together with normalized enrichment:

```bash
PYTHONPATH=src .venv/bin/python -m linkedin_index.ingest \
  --dataset /path/to/crawl-dataset.json \
  --normalized /path/to/runtime/normalized.json \
  --qdrant-url http://localhost:6333
```

## 2026-09-10 validation

The current source loader parsed 88/88 source files and normalized 23,277 entities: 16,887 people, 5,433 companies, 949 schools, and 8 posts. The old runtime had skipped 16 legacy Excel XML files and contained canonical-equivalent person URLs; the rebuilt 500-profile seed sample has no canonical duplicates.

Apify actor build `0.0.23` fixes request accounting so idle workers no longer inflate the `requested` denominator. A 20-person depth-zero canary produced exactly 20 records: 7 public SSR `200`, 12 deterministic `999`, and 1 `404`, all on one attempt. The run reported 100% accounting coverage while actual person-page fetch success was 35%.

The seven successful profiles exposed 309 unique next-hop LinkedIn URLs. Company-first expansion then produced 52/52 successful company pages. The first eight graph-discovered person pages produced 2/8 successful public SSR responses, confirming that current person links alone do not remove LinkedIn's person-page visibility wall. Company pages are therefore prioritized as reliable crawl hubs before person pages.
