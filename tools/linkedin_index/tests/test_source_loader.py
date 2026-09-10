from __future__ import annotations

import csv
import json
from datetime import datetime
from pathlib import Path

from openpyxl import Workbook

from linkedin_index.source_loader import (
    load_source_tree,
    normalize_sources,
    select_pilot_seeds,
)


def _write_csv(path: Path, rows: list[dict[str, object]]) -> None:
    fieldnames = list(rows[0])
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def test_load_source_tree_extracts_people_and_companies_and_preserves_fields(tmp_path: Path):
    source = tmp_path / "apollo.csv"
    _write_csv(
        source,
        [
            {
                "First Name": "Ada",
                "Last Name": "Lovelace",
                "Title": "VP Operations",
                "Company Name": "Analytical Engines",
                "Email": "ada@example.com",
                "Mobile Phone": "+1 514 555 0100",
                "Industry": "industrial automation",
                "LinkedIn Location": "Montreal, Quebec, Canada",
                "Person Linkedin Url": "http://linkedin.com/in/ada-lovelace/?trk=abc",
                "Company Linkedin Url": "https://linkedin.com/company/analytical-engines/",
                "Custom Score": "91",
            }
        ],
    )

    loaded = load_source_tree(tmp_path)
    assert loaded.report.files_read == 1
    assert loaded.report.rows_read == 1
    assert len(loaded.records) == 2

    by_url = {record.linkedin_url: record for record in loaded.records}
    person = by_url["https://www.linkedin.com/in/ada-lovelace"]
    company = by_url["https://www.linkedin.com/company/analytical-engines"]

    for record in (person, company):
        assert record.fields["First Name"] == "Ada"
        assert record.fields["Custom Score"] == "91"
        assert record.fields["name"] == "Ada Lovelace"
        assert record.fields["company"] == "Analytical Engines"
        assert record.fields["email"] == "ada@example.com"
        assert record.fields["phone"] == "+1 514 555 0100"
        assert record.fields["location"] == "Montreal, Quebec, Canada"
        assert record.fields["industry"] == "industrial automation"


def test_load_source_tree_reads_xlsx_and_reports_unreadable_files(tmp_path: Path):
    workbook = Workbook()
    sheet = workbook.active
    sheet.append(["Name", "LinkedIn URL", "Email"])
    sheet.append(["Grace Hopper", "https://www.linkedin.com/in/grace-hopper", "grace@example.com"])
    workbook.save(tmp_path / "people.xlsx")
    (tmp_path / "broken.xls").write_bytes(b"not-an-xls")

    loaded = load_source_tree(tmp_path)

    assert any(r.linkedin_url == "https://www.linkedin.com/in/grace-hopper" for r in loaded.records)
    assert loaded.report.files_total == 2
    assert loaded.report.files_read == 1
    assert loaded.report.files_failed == 1
    assert loaded.report.errors[0]["file"].endswith("broken.xls")


def test_load_source_tree_reads_excel_2003_xml_with_xls_extension(tmp_path: Path):
    xml = '''<?xml version="1.0"?>
<?mso-application progid="Excel.Sheet"?>
<Workbook xmlns="urn:schemas-microsoft-com:office:spreadsheet"
 xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet">
 <Worksheet ss:Name="Sheet1"><Table>
  <Row>
   <Cell><Data ss:Type="String">Name</Data></Cell>
   <Cell><Data ss:Type="String">LinkedIn URL</Data></Cell>
   <Cell><Data ss:Type="String">Email</Data></Cell>
  </Row>
  <Row>
   <Cell><Data ss:Type="String">XML Person</Data></Cell>
   <Cell><Data ss:Type="String">https://www.linkedin.com/in/xml-person</Data></Cell>
   <Cell><Data ss:Type="String">xml@example.com</Data></Cell>
  </Row>
 </Table></Worksheet>
</Workbook>'''
    (tmp_path / "legacy.xls").write_text(xml, encoding="utf-8")

    loaded = load_source_tree(tmp_path)
    assert loaded.report.files_read == 1
    assert loaded.report.files_failed == 0
    assert loaded.records[0].linkedin_url == "https://www.linkedin.com/in/xml-person"
    assert loaded.records[0].fields["email"] == "xml@example.com"


def test_normalize_sources_reports_dedup_contact_coverage_and_conflicts(tmp_path: Path):
    _write_csv(
        tmp_path / "one.csv",
        [
            {
                "Name": "Example Person",
                "Title": "VP Operations",
                "Email": "person@example.com",
                "Person Linkedin Url": "https://linkedin.com/in/example",
            }
        ],
    )
    _write_csv(
        tmp_path / "two.csv",
        [
            {
                "Name": "Example Person",
                "Title": "SVP Operations",
                "Mobile Phone": "+1 514 555 0199",
                "Person Linkedin Url": "https://www.linkedin.com/in/example/",
            }
        ],
    )

    result = normalize_sources(tmp_path)
    assert result.report.source_records == 2
    assert result.report.unique_entities == 1
    assert result.report.people == 1
    assert result.report.records_with_email == 1
    assert result.report.records_with_phone == 1
    assert result.report.records_with_both == 1
    assert result.report.duplicates_merged == 1
    assert result.report.conflicting_fields >= 1

    entity = result.entities["https://www.linkedin.com/in/example"]
    assert entity.unique_value("email") == "person@example.com"
    assert entity.unique_value("phone") == "+1 514 555 0199"
    assert entity.unique_value("title") is None


def test_select_pilot_seeds_prefers_people_and_is_deterministic(tmp_path: Path):
    _write_csv(
        tmp_path / "seed.csv",
        [
            {
                "Name": "B Person",
                "Email": "b@example.com",
                "Person Linkedin Url": "https://linkedin.com/in/b-person",
                "Company Linkedin Url": "https://linkedin.com/company/b-company",
            },
            {
                "Name": "A Person",
                "Person Linkedin Url": "https://linkedin.com/in/a-person",
                "Company Linkedin Url": "https://linkedin.com/company/a-company",
            },
        ],
    )
    result = normalize_sources(tmp_path)

    assert select_pilot_seeds(result.entities, limit=2) == [
        "https://www.linkedin.com/in/b-person",
        "https://www.linkedin.com/in/a-person",
    ]


def test_select_pilot_seeds_collapses_apollo_aliases_and_prefers_descriptive_slug():
    from linkedin_index.normalize import SourceRecord, merge_records

    entities = merge_records(
        [
            SourceRecord(
                source_file="apollo.csv",
                row_number=2,
                linkedin_url="https://www.linkedin.com/in/n",
                fields={
                    "name": "Narine Ter-Stepanyan",
                    "email": "narine@example.com",
                    "Apollo Contact Id": "apollo-1",
                },
            ),
            SourceRecord(
                source_file="apollo.csv",
                row_number=3,
                linkedin_url="https://www.linkedin.com/in/narine-ter-stepanyan-75161527",
                fields={
                    "name": "Narine Ter-Stepanyan",
                    "email": "narine@example.com",
                    "Apollo Contact Id": "apollo-1",
                },
            ),
            SourceRecord(
                source_file="apollo.csv",
                row_number=4,
                linkedin_url="https://www.linkedin.com/in/other-person",
                fields={
                    "name": "Other Person",
                    "email": "other@example.com",
                    "Apollo Contact Id": "apollo-2",
                },
            ),
        ]
    )

    assert select_pilot_seeds(entities, limit=3) == [
        "https://www.linkedin.com/in/narine-ter-stepanyan-75161527",
        "https://www.linkedin.com/in/other-person",
    ]


def test_cli_outputs_lossless_normalized_json(tmp_path: Path):
    _write_csv(
        tmp_path / "seed.csv",
        [
            {
                "Name": "Example Person",
                "Email": "example@example.com",
                "Person Linkedin Url": "https://linkedin.com/in/example",
                "Arbitrary Custom Column": "keep-me",
            }
        ],
    )
    result = normalize_sources(tmp_path)
    payload = result.to_jsonable()
    round_tripped = json.loads(json.dumps(payload))

    entity = round_tripped["entities"]["https://www.linkedin.com/in/example"]
    assert entity["fields"]["Arbitrary Custom Column"][0]["value"] == "keep-me"
    assert round_tripped["report"]["unique_entities"] == 1


def test_json_output_serializes_spreadsheet_dates_without_dropping_provenance(tmp_path: Path):
    workbook = Workbook()
    sheet = workbook.active
    sheet.append(["Name", "LinkedIn URL", "Last Contacted"])
    sheet.append([
        "Date Person",
        "https://www.linkedin.com/in/date-person",
        datetime(2026, 9, 3, 14, 30),
    ])
    workbook.save(tmp_path / "dated.xlsx")

    payload = normalize_sources(tmp_path).to_jsonable()
    encoded = json.dumps(payload)
    decoded = json.loads(encoded)
    value = decoded["entities"]["https://www.linkedin.com/in/date-person"]["fields"]["Last Contacted"][0]
    assert value["value"].startswith("2026-09-03 14:30:00")
    assert value["source_file"] == "dated.xlsx"
