from __future__ import annotations

import json
from pathlib import Path

from linkedin_index.ingest import (
    build_all_documents,
    build_documents,
    dense_candidates,
    documents_for_ingest,
    load_apify_datasets,
    load_normalized_entities,
    make_entity_type_filter,
)


def test_load_apify_datasets_accepts_json_arrays_and_jsonl(tmp_path: Path):
    array_path = tmp_path / "a.json"
    array_path.write_text(json.dumps([{"url": "https://linkedin.com/in/a", "status": 200}]), encoding="utf-8")
    jsonl_path = tmp_path / "b.jsonl"
    jsonl_path.write_text(
        "\n".join(
            [
                json.dumps({"url": "https://linkedin.com/in/b", "status": 200}),
                json.dumps({"url": "https://linkedin.com/in/c", "status": 999}),
            ]
        ),
        encoding="utf-8",
    )

    rows = load_apify_datasets([array_path, jsonl_path])
    assert [row["url"] for row in rows] == [
        "https://linkedin.com/in/a",
        "https://linkedin.com/in/b",
        "https://linkedin.com/in/c",
    ]


def test_load_normalized_entities_reconstructs_provenance(tmp_path: Path):
    payload = {
        "entities": {
            "https://www.linkedin.com/in/example": {
                "url": "https://www.linkedin.com/in/example",
                "entity_type": "person",
                "fields": {
                    "email": [
                        {"value": "person@example.com", "source_file": "apollo.csv", "row_number": 4}
                    ]
                },
                "source_records": [
                    {
                        "source_file": "apollo.csv",
                        "row_number": 4,
                        "linkedin_url": "https://www.linkedin.com/in/example",
                        "fields": {"Email": "person@example.com"},
                    }
                ],
            }
        }
    }
    path = tmp_path / "normalized.json"
    path.write_text(json.dumps(payload), encoding="utf-8")

    entities = load_normalized_entities(path)
    entity = entities["https://www.linkedin.com/in/example"]
    assert entity.unique_value("email") == "person@example.com"
    assert entity.source_records[0].source_file == "apollo.csv"


def test_build_documents_skips_failures_and_joins_enrichment(tmp_path: Path):
    normalized = {
        "entities": {
            "https://www.linkedin.com/in/example": {
                "url": "https://www.linkedin.com/in/example",
                "entity_type": "person",
                "fields": {
                    "name": [{"value": "Example Person", "source_file": "a.csv", "row_number": 2}],
                    "email": [{"value": "person@example.com", "source_file": "a.csv", "row_number": 2}],
                },
                "source_records": [
                    {
                        "source_file": "a.csv",
                        "row_number": 2,
                        "linkedin_url": "https://www.linkedin.com/in/example",
                        "fields": {"Name": "Example Person", "Email": "person@example.com"},
                    }
                ],
            }
        }
    }
    path = tmp_path / "normalized.json"
    path.write_text(json.dumps(normalized), encoding="utf-8")
    entities = load_normalized_entities(path)

    documents, stats = build_documents(
        [
            {
                "url": "https://linkedin.com/in/example/",
                "status": 200,
                "fetched": True,
                "type": "person",
                "title": "Example Person | LinkedIn",
                "text": "VP of food safety operations",
            },
            {"url": "https://linkedin.com/in/blocked", "status": 999, "fetched": False},
        ],
        entities,
    )

    assert stats.rows_seen == 2
    assert stats.documents_built == 1
    assert stats.failed_crawl_rows == 1
    assert stats.enrichment_matches == 1
    assert documents[0].payload["email"] == "person@example.com"
    assert "food safety operations" in documents[0].text


def test_build_all_documents_indexes_every_enrichment_entity_then_overlays_scraped():
    from linkedin_index.normalize import SourceRecord, merge_records

    entities = merge_records(
        [
            SourceRecord(
                source_file="source.csv",
                row_number=1,
                linkedin_url="https://www.linkedin.com/in/a",
                fields={"name": "A Person", "company": "A Co"},
            ),
            SourceRecord(
                source_file="source.csv",
                row_number=2,
                linkedin_url="https://www.linkedin.com/in/b",
                fields={"name": "B Person", "company": "B Co"},
            ),
        ]
    )
    rows = [
        {
            "url": "https://www.linkedin.com/in/a",
            "status": 200,
            "fetched": True,
            "type": "person",
            "title": "A Person - Current LinkedIn Title",
            "text": "fresh public profile",
        },
        {"url": "https://www.linkedin.com/in/b", "status": 999, "fetched": False},
    ]

    documents, stats = build_all_documents(rows, entities)
    by_url = {document.url: document for document in documents}

    assert set(by_url) == {
        "https://www.linkedin.com/in/a",
        "https://www.linkedin.com/in/b",
    }
    assert "fresh public profile" in by_url["https://www.linkedin.com/in/a"].text
    assert by_url["https://www.linkedin.com/in/a"].payload["content_sources"] == ["enrichment", "public_ssr"]
    assert by_url["https://www.linkedin.com/in/b"].payload["content_sources"] == ["enrichment"]
    assert stats.documents_built == 2
    assert stats.enrichment_only_documents == 1


def test_dense_candidates_select_only_public_ssr_augmented_documents():
    from linkedin_index.corpus import IndexDocument

    documents = [
        IndexDocument(
            url="https://www.linkedin.com/in/base",
            text="base",
            payload={"content_sources": ["enrichment"]},
        ),
        IndexDocument(
            url="https://www.linkedin.com/in/public",
            text="public",
            payload={"content_sources": ["enrichment", "public_ssr"]},
        ),
    ]

    assert [document.url for document in dense_candidates(documents)] == [
        "https://www.linkedin.com/in/public"
    ]


def test_documents_for_ingest_defaults_to_pages_that_were_actually_crawled(tmp_path: Path):
    normalized = {
        "entities": {
            "https://www.linkedin.com/in/crawled": {
                "url": "https://www.linkedin.com/in/crawled",
                "entity_type": "person",
                "fields": {},
                "source_records": [],
            },
            "https://www.linkedin.com/in/enrichment-only": {
                "url": "https://www.linkedin.com/in/enrichment-only",
                "entity_type": "person",
                "fields": {},
                "source_records": [],
            },
        }
    }
    path = tmp_path / "normalized.json"
    path.write_text(json.dumps(normalized), encoding="utf-8")
    entities = load_normalized_entities(path)
    rows = [
        {
            "url": "https://www.linkedin.com/in/crawled",
            "status": 200,
            "fetched": True,
            "type": "person",
            "text": "crawled public profile",
        }
    ]

    crawl_only, _ = documents_for_ingest(rows, entities)
    with_enrichment_only, _ = documents_for_ingest(
        rows,
        entities,
        include_enrichment_only=True,
    )

    assert [document.url for document in crawl_only] == [
        "https://www.linkedin.com/in/crawled"
    ]
    assert {document.url for document in with_enrichment_only} == {
        "https://www.linkedin.com/in/crawled",
        "https://www.linkedin.com/in/enrichment-only",
    }


def test_make_entity_type_filter_uses_exact_payload_match():
    filter_ = make_entity_type_filter("person")
    condition = filter_.must[0]
    assert condition.key == "entity_type"
    assert condition.match.value == "person"
