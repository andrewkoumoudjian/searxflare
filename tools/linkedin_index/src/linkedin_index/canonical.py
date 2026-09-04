from __future__ import annotations

from urllib.parse import urlsplit, urlunsplit


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

    # Preserve the path exactly as supplied so percent-encoded slug bytes are stable.
    return urlunsplit(("https", "www.linkedin.com", path, "", ""))

