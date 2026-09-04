from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Any
from uuid import NAMESPACE_URL, uuid5

from .canonical import canonical_linkedin_url
from .normalize import NormalizedEntity


@dataclass(frozen=True)
class IndexDocument:
    url: str
    text: str
    payload: dict[str, Any]


def point_id(url: str) -> str:
    canonical = canonical_linkedin_url(url)
    if not canonical:
        raise ValueError(f"invalid LinkedIn URL: {url!r}")
    return str(uuid5(NAMESPACE_URL, canonical))


def _unique(entity: NormalizedEntity | None, field_name: str) -> Any | None:
    return entity.unique_value(field_name) if entity is not None else None


def _enrichment_payload(entity: NormalizedEntity | None) -> dict[str, list[dict[str, Any]]]:
    if entity is None:
        return {}
    return {
        name: [asdict(value) for value in values]
        for name, values in entity.fields.items()
    }


def build_document(scraped: dict[str, Any], enrichment: NormalizedEntity | None) -> IndexDocument:
    url = canonical_linkedin_url(str(scraped.get("url") or (enrichment.url if enrichment else "")))
    if not url:
        raise ValueError("scraped record does not contain a supported LinkedIn URL")

    entity_type = str(scraped.get("type") or (enrichment.entity_type if enrichment else "unknown"))
    source_files = sorted({record.source_file for record in enrichment.source_records}) if enrichment else []

    convenience = {
        "name": _unique(enrichment, "name"),
        "company": _unique(enrichment, "company"),
        "email": _unique(enrichment, "email"),
        "phone": _unique(enrichment, "phone"),
        "location": _unique(enrichment, "location"),
        "industry": _unique(enrichment, "industry"),
    }

    payload: dict[str, Any] = {
        "url": url,
        "entity_type": entity_type,
        **convenience,
        "fetched_at": scraped.get("fetchedAt") or scraped.get("scrapedAt"),
        "source_files": source_files,
        "scraped": dict(scraped),
        "enrichment": _enrichment_payload(enrichment),
    }

    sections: list[str] = []
    for key in ("name", "company", "email", "phone", "location", "industry"):
        value = convenience[key]
        if value is not None:
            sections.append(f"{key}: {value}")
    for key in ("title", "description"):
        value = scraped.get(key)
        if value:
            sections.append(str(value))
    for key in ("h1s", "h2s"):
        values = scraped.get(key) or []
        sections.extend(str(value) for value in values if value)
    for key in ("text", "markdown"):
        value = scraped.get(key)
        if value:
            sections.append(str(value))

    # Preserve ordering while avoiding exact duplicated sections.
    deduped = list(dict.fromkeys(sections))
    return IndexDocument(url=url, text="\n\n".join(deduped), payload=payload)

