from __future__ import annotations

import unicodedata
from urllib.parse import quote, unquote, urlsplit, urlunsplit


_SUPPORTED_ROOTS = {"in", "company", "school", "pulse", "posts", "showcase"}


def canonical_linkedin_url(value: str) -> str | None:
    """Return the canonical public LinkedIn entity URL, or None when unsupported."""
    if not isinstance(value, str):
        return None
    raw = value.strip()
    if not raw:
        return None
    try:
        parsed = urlsplit(raw)
    except ValueError:
        return None

    if parsed.scheme.lower() not in {"http", "https"}:
        return None
    if parsed.username or parsed.password:
        return None

    host = (parsed.hostname or "").lower().rstrip(".")
    if host == "linkedin.com":
        pass
    elif host.endswith(".linkedin.com"):
        label = host[: -len(".linkedin.com")]
        if not label or "." in label:
            return None
    else:
        return None

    path = parsed.path.rstrip("/")
    parts = path.split("/")
    if len(parts) != 3 or parts[0] != "" or parts[1] not in _SUPPORTED_ROOTS or not parts[2]:
        return None

    # Normalize one entity slug without decoding path separators into structure.
    # This collapses Unicode and percent-encoded aliases and canonicalizes percent
    # escapes to uppercase hex, which LinkedIn serves more consistently.
    slug = quote(unicodedata.normalize("NFC", unquote(parts[2])), safe="._~-")
    normalized_path = f"/{parts[1]}/{slug}"
    return urlunsplit(("https", "www.linkedin.com", normalized_path, "", ""))
