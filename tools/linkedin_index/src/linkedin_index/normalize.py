from __future__ import annotations

from dataclasses import dataclass, field
import re
from typing import Any, Iterable
from urllib.parse import unquote

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


def _normalized_field_name(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", value.casefold())


def _stable_person_ids(entity: NormalizedEntity) -> set[str]:
    stable_keys = {"apollocontactid", "apollorecordid"}
    ids: set[str] = set()
    for field_name, values in entity.fields.items():
        if _normalized_field_name(field_name) not in stable_keys:
            continue
        for item in values:
            if _is_nonempty(item.value):
                ids.add(str(item.value).strip())
    return ids


def _name_tokens(entity: NormalizedEntity) -> set[str]:
    raw_values = entity.fields.get("name", [])
    for item in raw_values:
        if not _is_nonempty(item.value):
            continue
        return {
            token
            for token in re.findall(r"[a-z0-9]+", str(item.value).casefold())
            if len(token) > 1
        }
    return set()


def _alias_quality(url: str, entity: NormalizedEntity) -> tuple[int, int, int, str]:
    slug = unquote(url.rsplit("/", 1)[-1]).casefold()
    slug_tokens = set(re.findall(r"[a-z0-9]+", slug))
    name_tokens = _name_tokens(entity)
    overlap = len(name_tokens & slug_tokens)
    malformed_penalty = int(len(slug) <= 3)
    return (overlap, -malformed_penalty, len(slug), url)


def collapse_person_aliases(
    entities: dict[str, NormalizedEntity],
) -> dict[str, NormalizedEntity]:
    """Merge person URLs that share a stable Apollo contact/record identity."""
    result = dict(entities)
    person_urls = [url for url, entity in result.items() if entity.entity_type == "person"]

    parent = {url: url for url in person_urls}

    def find(url: str) -> str:
        while parent[url] != url:
            parent[url] = parent[parent[url]]
            url = parent[url]
        return url

    def union(left: str, right: str) -> None:
        root_left, root_right = find(left), find(right)
        if root_left != root_right:
            parent[root_right] = root_left

    first_by_identity: dict[str, str] = {}
    for url in person_urls:
        for stable_id in sorted(_stable_person_ids(result[url])):
            prior = first_by_identity.get(stable_id)
            if prior is None:
                first_by_identity[stable_id] = url
            else:
                union(prior, url)

    components: dict[str, list[str]] = {}
    for url in person_urls:
        components.setdefault(find(url), []).append(url)

    for urls in components.values():
        if len(urls) < 2:
            continue
        representative_url = max(urls, key=lambda url: _alias_quality(url, result[url]))
        representative = result[representative_url]

        for alias_url in sorted(urls):
            if alias_url == representative_url:
                continue
            alias = result[alias_url]
            for field_name, values in alias.fields.items():
                target = representative.fields.setdefault(field_name, [])
                for value in values:
                    if value not in target:
                        target.append(value)
            for source_record in alias.source_records:
                if source_record not in representative.source_records:
                    representative.source_records.append(source_record)

            provenance = alias.source_records[0] if alias.source_records else None
            aliases = representative.fields.setdefault("linkedin_url_alias", [])
            candidate = ProvenancedValue(
                value=alias_url,
                source_file=provenance.source_file if provenance else "",
                row_number=provenance.row_number if provenance else 0,
            )
            if candidate not in aliases:
                aliases.append(candidate)
            result.pop(alias_url, None)

    return result
