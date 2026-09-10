from __future__ import annotations

import argparse
import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, Sequence

from qdrant_client import models

from .canonical import canonical_linkedin_url
from .corpus import IndexDocument, build_document
from .normalize import NormalizedEntity, ProvenancedValue, SourceRecord
from .qdrant_index import LinkedInIndex


@dataclass
class IngestStats:
    rows_seen: int = 0
    failed_crawl_rows: int = 0
    invalid_rows: int = 0
    documents_built: int = 0
    enrichment_matches: int = 0
    enrichment_only_documents: int = 0
    inserted: int = 0
    elapsed_seconds: float = 0.0
    documents_per_second: float = 0.0


def _load_jsonish(path: Path) -> list[dict[str, Any]]:
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        return []
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        rows: list[dict[str, Any]] = []
        for line in text.splitlines():
            line = line.strip()
            if not line:
                continue
            item = json.loads(line)
            if not isinstance(item, dict):
                raise ValueError(f"JSONL row in {path} was not an object")
            rows.append(item)
        return rows
    if isinstance(payload, list):
        if not all(isinstance(item, dict) for item in payload):
            raise ValueError(f"JSON array in {path} must contain only objects")
        return [dict(item) for item in payload]
    if isinstance(payload, dict):
        return [payload]
    raise ValueError(f"unsupported JSON payload in {path}")


def load_apify_datasets(paths: Iterable[str | Path]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for raw in paths:
        rows.extend(_load_jsonish(Path(raw)))
    return rows


def load_normalized_entities(path: str | Path) -> dict[str, NormalizedEntity]:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    raw_entities = payload.get("entities") if isinstance(payload, dict) else None
    if not isinstance(raw_entities, dict):
        raise ValueError("normalized payload must contain an 'entities' object")

    entities: dict[str, NormalizedEntity] = {}
    for raw_url, raw_entity in raw_entities.items():
        if not isinstance(raw_entity, dict):
            continue
        url = canonical_linkedin_url(str(raw_entity.get("url") or raw_url))
        if not url:
            continue
        fields: dict[str, list[ProvenancedValue]] = {}
        raw_fields = raw_entity.get("fields") or {}
        if isinstance(raw_fields, dict):
            for name, raw_values in raw_fields.items():
                values: list[ProvenancedValue] = []
                if isinstance(raw_values, list):
                    for raw_value in raw_values:
                        if not isinstance(raw_value, dict):
                            continue
                        values.append(
                            ProvenancedValue(
                                value=raw_value.get("value"),
                                source_file=str(raw_value.get("source_file") or ""),
                                row_number=int(raw_value.get("row_number") or 0),
                            )
                        )
                fields[str(name)] = values

        source_records: list[SourceRecord] = []
        raw_records = raw_entity.get("source_records") or []
        if isinstance(raw_records, list):
            for raw_record in raw_records:
                if not isinstance(raw_record, dict):
                    continue
                record_url = canonical_linkedin_url(str(raw_record.get("linkedin_url") or url)) or url
                source_records.append(
                    SourceRecord(
                        source_file=str(raw_record.get("source_file") or ""),
                        row_number=int(raw_record.get("row_number") or 0),
                        linkedin_url=record_url,
                        fields=dict(raw_record.get("fields") or {}),
                    )
                )
        entities[url] = NormalizedEntity(
            url=url,
            entity_type=str(raw_entity.get("entity_type") or "unknown"),
            fields=fields,
            source_records=source_records,
        )
    return entities


def _successful_row(row: dict[str, Any]) -> bool:
    status = row.get("status")
    fetched = row.get("fetched")
    if status is not None:
        try:
            if int(status) != 200:
                return False
        except (TypeError, ValueError):
            return False
    if fetched is False:
        return False
    return True


def build_documents(
    rows: Sequence[dict[str, Any]],
    entities: dict[str, NormalizedEntity],
) -> tuple[list[IndexDocument], IngestStats]:
    stats = IngestStats(rows_seen=len(rows))
    documents: list[IndexDocument] = []
    seen_urls: set[str] = set()
    for row in rows:
        if not _successful_row(row):
            stats.failed_crawl_rows += 1
            continue
        url = canonical_linkedin_url(str(row.get("url") or ""))
        if not url:
            stats.invalid_rows += 1
            continue
        if url in seen_urls:
            continue
        seen_urls.add(url)
        enrichment = entities.get(url)
        try:
            document = build_document(row, enrichment)
        except ValueError:
            stats.invalid_rows += 1
            continue
        if enrichment is not None:
            stats.enrichment_matches += 1
        documents.append(document)
    stats.documents_built = len(documents)
    return documents, stats


def build_all_documents(
    rows: Sequence[dict[str, Any]],
    entities: dict[str, NormalizedEntity],
) -> tuple[list[IndexDocument], IngestStats]:
    scraped_documents, stats = build_documents(rows, entities)
    by_url = {document.url: document for document in scraped_documents}
    for url, entity in entities.items():
        if url in by_url:
            continue
        by_url[url] = build_document({}, entity)
        stats.enrichment_only_documents += 1
    documents = [by_url[url] for url in sorted(by_url)]
    stats.documents_built = len(documents)
    return documents, stats


def documents_for_ingest(
    rows: Sequence[dict[str, Any]],
    entities: dict[str, NormalizedEntity],
    *,
    include_enrichment_only: bool = False,
) -> tuple[list[IndexDocument], IngestStats]:
    """Build the crawl-backed index by default; enrichment-only points are opt-in."""
    if include_enrichment_only:
        return build_all_documents(rows, entities)
    return build_documents(rows, entities)


def dense_candidates(documents: Sequence[IndexDocument]) -> list[IndexDocument]:
    return [
        document
        for document in documents
        if "public_ssr" in (document.payload.get("content_sources") or [])
    ]


def make_entity_type_filter(entity_type: str | None) -> models.Filter | None:
    if not entity_type:
        return None
    return models.Filter(
        must=[
            models.FieldCondition(
                key="entity_type",
                match=models.MatchValue(value=entity_type),
            )
        ]
    )


def ingest_documents(
    index: LinkedInIndex,
    documents: Sequence[IndexDocument],
    stats: IngestStats | None = None,
    *,
    batch_size: int = 64,
    include_dense: bool = True,
) -> IngestStats:
    stats = stats or IngestStats(documents_built=len(documents))
    started = time.perf_counter()
    stats.inserted = index.upsert(
        documents,
        batch_size=batch_size,
        include_dense=include_dense,
    )
    stats.elapsed_seconds = time.perf_counter() - started
    stats.documents_per_second = (
        stats.inserted / stats.elapsed_seconds if stats.elapsed_seconds > 0 else 0.0
    )
    return stats


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Join Apify crawl output with enrichment and ingest into Qdrant")
    parser.add_argument("--dataset", type=Path, action="append", required=True)
    parser.add_argument("--normalized", type=Path, required=True)
    parser.add_argument("--qdrant-url", default="http://127.0.0.1:6333")
    parser.add_argument("--batch-size", type=int, default=64)
    parser.add_argument(
        "--include-enrichment-only",
        action="store_true",
        help="Also create points for enrichment records that were never successfully crawled",
    )
    parser.add_argument("--query", action="append", default=[])
    parser.add_argument("--entity-type", choices=["person", "company", "school", "post"])
    parser.add_argument("--limit", type=int, default=10)
    args = parser.parse_args(argv)

    entities = load_normalized_entities(args.normalized)
    rows = load_apify_datasets(args.dataset)
    documents, stats = documents_for_ingest(
        rows,
        entities,
        include_enrichment_only=args.include_enrichment_only,
    )
    index = LinkedInIndex(url=args.qdrant_url)
    stats = ingest_documents(index, documents, stats, batch_size=args.batch_size)
    result: dict[str, Any] = {"stats": stats.__dict__}
    if args.query:
        filter_ = make_entity_type_filter(args.entity_type)
        result["queries"] = {
            query: index.search(query, limit=args.limit, filters=filter_)
            for query in args.query
        }
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
