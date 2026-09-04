"""Router + UA map + full-content extraction (html + ld+json + markdown + text).

BFS child requests inherit the spoofed header set at enqueue time.
"""
from __future__ import annotations

import asyncio
import json
import random
import re
from urllib.parse import urljoin

from crawlee import Request
from crawlee.crawlers import BeautifulSoupCrawlingContext
from crawlee.router import Router

from apify import Actor

LINKEDIN_RE = re.compile(
    r"https?://(?:www\.)?linkedin\.com/(?:in|company|school|pulse|posts|showcase)/[^\"'#\s\?]+",
    re.I,
)

UA_MAP = {
    "claude-user": "Claude-User",
    "google": "Google",
    "openai": "OpenAI File Downloader",
    "xai": "XaiImageApiFetch/1.0",
    "gptbot": "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.2; +https://openai.com/gptbot)",
    "perplexity": "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)",
    "googlebot": "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    "chrome": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
}

UA_HEADERS = {
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.9",
}


def resolve_ua(spoof: str, custom: str = "") -> str:
    s = (spoof or "").strip().lower().replace("-", "_")
    if s == "custom" and custom:
        return custom.strip()
    return UA_MAP.get(s, UA_MAP["claude-user"])


router = Router[BeautifulSoupCrawlingContext]()
router._max_depth = 2  # type: ignore[attr-defined]
router._max_links = 64  # type: ignore[attr-defined]
router._headers = {**UA_HEADERS, "User-Agent": UA_MAP["claude-user"]}  # type: ignore[attr-defined]


@router.default_handler
async def handler(ctx: BeautifulSoupCrawlingContext) -> None:
    url = ctx.request.url
    depth = int(ctx.request.user_data.get("depth", 0) or 0)
    max_depth = int(router._max_depth)  # type: ignore[attr-defined]
    max_links = int(router._max_links)  # type: ignore[attr-defined]

    # human-ish pacing to avoid LinkedIn 429 bursts
    await asyncio.sleep(random.uniform(0.8, 2.0))

    soup = ctx.soup
    if soup is None:
        Actor.log.warning(f"no soup {url}")
        return

    title = soup.title.string.strip() if soup.title and soup.title.string else ""
    ld_jsons = []
    for s in soup.find_all("script", type="application/ld+json"):
        raw = s.string or s.get_text()
        if raw:
            try:
                ld_jsons.append(json.loads(raw))
            except Exception:
                ld_jsons.append({"raw": raw[:2000]})

    h1s = [h.get_text(" ", strip=True) for h in soup.find_all("h1")]
    h2s = [h.get_text(" ", strip=True) for h in soup.find_all("h2")][:10]
    m = soup.find("meta", attrs={"name": "description"})
    meta_desc = m["content"].strip() if m and m.get("content") else ""

    text = soup.get_text(" ", strip=True)[:262_144]
    try:
        from markdownify import markdownify as md

        markdown = md(str(soup), heading_style="ATX")[:1_048_576]
    except Exception:
        markdown = text[:1_048_576]
    html_snip = str(soup)[:1_048_576]

    if "/in/" in url:
        page_type = "person"
    elif "/company/" in url:
        page_type = "company"
    elif "/school/" in url:
        page_type = "school"
    elif "/pulse/" in url or "/posts/" in url:
        page_type = "post"
    else:
        page_type = "unknown"

    # detect authwall / 999 blocks so they are visible in the dataset
    blocked = ("linkedin.com/authwall" in url) or ("authwall" in (title or "")) or text.startswith("999")

    raw_links = []
    for a in soup.find_all("a", href=True):
        full = urljoin(url, a["href"]).split("#")[0].split("?")[0]
        if LINKEDIN_RE.match(full):
            raw_links.append(full)
    seen: set[str] = set()
    uniq_links = []
    for l in raw_links:
        if l not in seen:
            seen.add(l)
            uniq_links.append(l)
        if len(uniq_links) >= max_links:
            break

    record = {
        "url": url,
        "title": title,
        "type": page_type,
        "depth": depth,
        "blocked": blocked,
        "has_ld": int(len(ld_jsons) > 0),
        "has_person": int(any(isinstance(j, dict) and "Person" in json.dumps(j) for j in ld_jsons)),
        "h1s": h1s,
        "h2s": h2s,
        "description": meta_desc,
        "text": text,
        "markdown": markdown,
        "html": html_snip,
        "ldJson": ld_jsons,
        "linksFound": len(uniq_links),
        "rawLinks": len(raw_links),
        "text_len": len(text),
    }
    await ctx.push_data(record)

    # BFS: children inherit the same spoofed headers via Request.from_url
    if depth < max_depth and uniq_links and not blocked:
        reqs = [
            Request.from_url(u, headers=router._headers, user_data={"depth": depth + 1})  # type: ignore[attr-defined]
            for u in uniq_links
        ]
        await ctx.add_requests(reqs)
