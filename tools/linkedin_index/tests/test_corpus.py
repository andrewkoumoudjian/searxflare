from linkedin_index.corpus import build_document, point_id
from linkedin_index.normalize import SourceRecord, merge_records


def test_build_document_preserves_scraped_and_enrichment_payloads():
    entity = merge_records(
        [
            SourceRecord(
                source_file="apollo.csv",
                row_number=4,
                linkedin_url="https://linkedin.com/in/example",
                fields={
                    "name": "Example Person",
                    "email": "person@example.com",
                    "company": "Acme Foods",
                    "industry": "Food Production",
                    "arbitrary_source_field": "keep-me",
                },
            )
        ]
    )["https://www.linkedin.com/in/example"]
    scraped = {
        "url": "https://www.linkedin.com/in/example/",
        "type": "person",
        "title": "Example Person - Acme Foods | LinkedIn",
        "description": "VP Operations in food manufacturing",
        "h1s": ["Example Person"],
        "h2s": ["Experience", "Education"],
        "text": "Runs plant operations and food safety programs.",
        "markdown": "# Example Person\nRuns plant operations.",
        "html": "<html><body>full raw page</body></html>",
        "ldJson": [{"@type": "Person", "name": "Example Person"}],
        "fetchedAt": "2026-09-03T12:00:00Z",
    }

    document = build_document(scraped, entity)

    assert document.url == "https://www.linkedin.com/in/example"
    assert document.payload["scraped"]["html"] == scraped["html"]
    assert document.payload["enrichment"]["arbitrary_source_field"][0]["value"] == "keep-me"
    assert document.payload["email"] == "person@example.com"
    assert document.payload["company"] == "Acme Foods"
    assert "Food Production" in document.text
    assert "food safety programs" in document.text


def test_point_id_is_stable_for_equivalent_canonical_urls():
    assert point_id("https://linkedin.com/in/example/?trk=x") == point_id(
        "https://www.linkedin.com/in/example"
    )

