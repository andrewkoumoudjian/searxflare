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
                    "title": "VP Operations",
                    "email": "person@example.com",
                    "company": "Acme Foods",
                    "industry": "Food Production",
                    "Seniority": "VP",
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
    assert document.payload["title"] == "VP Operations"
    assert "Food Production" in document.text
    assert "VP Operations" in document.text
    assert "Seniority: VP" in document.text
    assert "food safety programs" in document.text


def test_build_document_can_index_enrichment_without_scraped_page():
    entity = merge_records(
        [
            SourceRecord(
                source_file="apollo.csv",
                row_number=7,
                linkedin_url="https://linkedin.com/in/enrichment-only",
                fields={
                    "name": "Enrichment Only",
                    "title": "Chief Financial Officer",
                    "company": "Example Holdings",
                    "location": "Montreal, Quebec, Canada",
                    "Department": "Finance",
                },
            )
        ]
    )["https://www.linkedin.com/in/enrichment-only"]

    document = build_document({}, entity)

    assert document.url == "https://www.linkedin.com/in/enrichment-only"
    assert document.payload["scraped"] == {}
    assert document.payload["content_sources"] == ["enrichment"]
    assert "Chief Financial Officer" in document.text
    assert "Department: Finance" in document.text


def test_build_document_exposes_collapsed_linkedin_aliases():
    entity = merge_records(
        [
            SourceRecord(
                source_file="source.csv",
                row_number=1,
                linkedin_url="https://www.linkedin.com/in/canonical-person",
                fields={"name": "Canonical Person"},
            )
        ]
    )["https://www.linkedin.com/in/canonical-person"]
    entity.source_records.append(
        SourceRecord(
            source_file="old.csv",
            row_number=9,
            linkedin_url="https://www.linkedin.com/in/old-person-alias",
            fields={"name": "Canonical Person"},
        )
    )

    document = build_document({}, entity)

    assert document.payload["aliases"] == ["https://www.linkedin.com/in/old-person-alias"]


def test_point_id_is_stable_for_equivalent_canonical_urls():
    assert point_id("https://linkedin.com/in/example/?trk=x") == point_id(
        "https://www.linkedin.com/in/example"
    )


def test_crawled_company_identity_overrides_person_fields_from_shared_enrichment_row():
    entity = merge_records(
        [
            SourceRecord(
                source_file="apollo.csv",
                row_number=9,
                linkedin_url="https://www.linkedin.com/company/alasko-foods-moov",
                fields={
                    "name": "Narine Ter-Stepanyan",
                    "company": "Alasko",
                    "email": "narine@example.com",
                },
            )
        ]
    )["https://www.linkedin.com/company/alasko-foods-moov"]
    scraped = {
        "url": "https://www.linkedin.com/company/alasko-foods-moov",
        "status": 200,
        "fetched": True,
        "type": "company",
        "title": "Alasko | LinkedIn",
        "h1s": ["Alasko"],
        "description": "Frozen fruit and vegetable company",
    }

    document = build_document(scraped, entity)

    assert document.payload["name"] == "Alasko"
    assert document.payload["email"] is None
    assert document.payload["company"] == "Alasko"
