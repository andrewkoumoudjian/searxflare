from linkedin_index.normalize import SourceRecord, merge_records


def test_merge_records_preserves_arbitrary_fields_and_contact_data():
    entities = merge_records(
        [
            SourceRecord(
                source_file="apollo.csv",
                row_number=7,
                linkedin_url="https://linkedin.com/in/example/?trk=x",
                fields={
                    "name": "Example Person",
                    "email": "person@example.com",
                    "phone": "+1 514 555 0100",
                    "apollo_id": "abc123",
                    "custom_score": 91,
                },
            )
        ]
    )

    entity = entities["https://www.linkedin.com/in/example"]
    assert entity.entity_type == "person"
    assert entity.unique_value("email") == "person@example.com"
    assert entity.unique_value("phone") == "+1 514 555 0100"
    assert entity.unique_value("custom_score") == 91
    assert entity.fields["apollo_id"][0].source_file == "apollo.csv"
    assert entity.fields["apollo_id"][0].row_number == 7


def test_merge_records_keeps_conflicting_values_with_provenance():
    entities = merge_records(
        [
            SourceRecord(
                source_file="phantombuster.csv",
                row_number=2,
                linkedin_url="https://www.linkedin.com/in/example",
                fields={"title": "VP Operations", "company": "Acme"},
            ),
            SourceRecord(
                source_file="apollo.csv",
                row_number=9,
                linkedin_url="https://www.linkedin.com/in/example/",
                fields={"title": "SVP Operations", "company": "Acme"},
            ),
        ]
    )

    entity = entities["https://www.linkedin.com/in/example"]
    assert entity.unique_value("company") == "Acme"
    assert entity.unique_value("title") is None
    assert [(v.value, v.source_file) for v in entity.fields["title"]] == [
        ("VP Operations", "phantombuster.csv"),
        ("SVP Operations", "apollo.csv"),
    ]


def test_merge_records_skips_rows_without_a_valid_linkedin_url():
    entities = merge_records(
        [
            SourceRecord(
                source_file="bad.csv",
                row_number=1,
                linkedin_url="https://example.com/person",
                fields={"name": "Bad"},
            )
        ]
    )
    assert entities == {}

