from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterable

from .canonical import canonical_linkedin_url


@dataclass(frozen=True)
class SourceRecord:
    source_file: str
    row_number: int
    linkedin_url: str
    fields: dict[str, Any]


@dataclass(frozen=True)
class ProvenancedValue:
    value: Any
    source_file: str
    row_number: int


@dataclass
class NormalizedEntity:
    url: str
    entity_type: str
    fields: dict[str, list[ProvenancedValue]] = field(default_factory=dict)
    source_records: list[SourceRecord] = field(default_factory=list)

    def unique_value(self, field_name: str) -> Any | None:
        values = self.fields.get(field_name, [])
        distinct: list[Any] = []
        for item in values:
            if item.value not in distinct:
                distinct.append(item.value)
        return distinct[0] if len(distinct) == 1 else None


def _entity_type(url: str) -> str:
    root = urlsplit_path_root(url)
    return {
        "in": "person",
        "company": "company",
        "school": "school",
        "pulse": "post",
        "posts": "post",
        "showcase": "company",
    }.get(root, "unknown")


def urlsplit_path_root(url: str) -> str:
    path = url.split("linkedin.com/", 1)[-1]
    return path.split("/", 1)[0]


def _is_nonempty(value: Any) -> bool:
    if value is None:
        return False
    if isinstance(value, str):
        return bool(value.strip())
    return True


def merge_records(rows: Iterable[SourceRecord]) -> dict[str, NormalizedEntity]:
    entities: dict[str, NormalizedEntity] = {}
    for row in rows:
        url = canonical_linkedin_url(row.linkedin_url)
        if not url:
            continue
        entity = entities.setdefault(url, NormalizedEntity(url=url, entity_type=_entity_type(url)))
        entity.source_records.append(row)
        for key, value in row.fields.items():
            if not _is_nonempty(value):
                continue
            values = entity.fields.setdefault(str(key), [])
            candidate = ProvenancedValue(value=value, source_file=row.source_file, row_number=row.row_number)
            if candidate not in values:
                values.append(candidate)
    return entities

