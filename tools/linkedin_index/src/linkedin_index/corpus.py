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


def _text_value(value: Any) -> str | None:
    if value is None:
        return None
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (str, int, float)):
        text = str(value).strip()
        return text[:1000] if text else None
    return None


def _enrichment_sections(entity: NormalizedEntity | None) -> list[str]:
    if entity is None:
        return []
    sections: list[str] = []
    for field_name in sorted(entity.fields, key=str.casefold):
        seen: set[str] = set()
        values: list[str] = []
        for item in entity.fields[field_name]:
            text = _text_value(item.value)
            if not text or text in seen:
                continue
            seen.add(text)
            values.append(text)
            if len(values) >= 8:
                break
        if values:
            sections.append(f"{field_name}: {' | '.join(values)}")
    return sections


def _scraped_heading(scraped: dict[str, Any]) -> str | None:
    h1s = scraped.get("h1s") or []
    if isinstance(h1s, list):
        for value in h1s:
            text = _text_value(value)
            if text:
                return text
    return None


def build_document(scraped: dict[str, Any], enrichment: NormalizedEntity | None) -> IndexDocument:
    url = canonical_linkedin_url(str(scraped.get("url") or (enrichment.url if enrichment else "")))
    if not url:
        raise ValueError("scraped record does not contain a supported LinkedIn URL")

    entity_type = str(scraped.get("type") or (enrichment.entity_type if enrichment else "unknown"))
    source_files = sorted({record.source_file for record in enrichment.source_records}) if enrichment else []
    aliases = []
    if enrichment:
        aliases = sorted(
            {
                candidate
                for record in enrichment.source_records
                if (candidate := canonical_linkedin_url(record.linkedin_url))
                and candidate != url
            }
        )

    scraped_name = _scraped_heading(scraped)
    enrichment_name = _unique(enrichment, "name")
    enrichment_company = _unique(enrichment, "company")
    if entity_type == "person":
        convenience = {
            "name": scraped_name or enrichment_name,
            "title": _unique(enrichment, "title"),
            "company": enrichment_company,
            "email": _unique(enrichment, "email"),
            "phone": _unique(enrichment, "phone"),
            "location": _unique(enrichment, "location"),
            "industry": _unique(enrichment, "industry"),
        }
    elif entity_type in {"company", "showcase"}:
        company_name = scraped_name or enrichment_company or enrichment_name
        convenience = {
            "name": company_name,
            "title": None,
            "company": company_name,
            "email": None,
            "phone": None,
            "location": _unique(enrichment, "location"),
            "industry": _unique(enrichment, "industry"),
        }
    elif entity_type == "school":
        convenience = {
            "name": scraped_name or enrichment_name,
            "title": None,
            "company": None,
            "email": None,
            "phone": None,
            "location": _unique(enrichment, "location"),
            "industry": None,
        }
    else:
        convenience = {
            "name": None,
            "title": None,
            "company": None,
            "email": None,
            "phone": None,
            "location": None,
            "industry": None,
        }

    payload: dict[str, Any] = {
        "url": url,
        "entity_type": entity_type,
        **convenience,
        "fetched_at": scraped.get("fetchedAt") or scraped.get("scrapedAt"),
        "source_files": source_files,
        "aliases": aliases,
        "scraped": dict(scraped),
        "enrichment": _enrichment_payload(enrichment),
        "content_sources": [
            source
            for source, present in (
                ("enrichment", enrichment is not None),
                ("public_ssr", bool(scraped)),
            )
            if present
        ],
    }

    sections: list[str] = []
    for key in ("name", "title", "company", "email", "phone", "location", "industry"):
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
    sections.extend(_enrichment_sections(enrichment))
    for key in ("text", "markdown"):
        value = scraped.get(key)
        if value:
            sections.append(str(value))

    # Preserve ordering while avoiding exact duplicated sections.
    deduped = list(dict.fromkeys(sections))
    return IndexDocument(url=url, text="\n\n".join(deduped), payload=payload)
