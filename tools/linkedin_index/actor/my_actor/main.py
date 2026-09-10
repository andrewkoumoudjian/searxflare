"""LinkedIn crawler — Apify-only, byte-for-byte searxflare UA config, 100% coverage.

Every canonical seed URL gets a dataset record: either full SSR extraction (status 200)
or a failure record with final status code. UA matrix rotates across retry attempts.
"""
from __future__ import annotations

import asyncio
import json
import random
import re
import time
import unicodedata
from enum import Enum
from urllib.parse import quote, unquote, urljoin, urlsplit, urlunsplit

import httpx
from bs4 import BeautifulSoup

from apify import Actor

# ------------------------- EXACT searxflare UA config (docs/ua-spoof-experiment.md)

UA_MATRIX = [
    ("claude",     "Claude-User"),
    ("gemini",     "Google"),
    ("openai_fd",  "OpenAI File Downloader"),
    ("xai",        "XaiImageApiFetch/1.0 (Linux; x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.3"),
    ("gptbot",     "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.2; +https://openai.com/gptbot)"),
    ("perplexity", "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)"),
]
UA_BY_KEY = dict(UA_MATRIX)


class RetryDecision(Enum):
    SUCCESS = "success"
    RETRY = "retry"
    TERMINAL = "terminal"


def retry_decision(status: int | None, attempt: int) -> RetryDecision:
    """Classify a fetch result without pretending deterministic 999s are UA-sensitive."""
    del attempt  # kept in the interface so bounded-attempt policies can evolve explicitly
    if status == 200:
        return RetryDecision.SUCCESS
    if status is None or status == 429 or (500 <= status < 600):
        return RetryDecision.RETRY
    return RetryDecision.TERMINAL


def request_headers(ua: str) -> dict[str, str]:
    return {
        "User-Agent": ua,
        "Accept": "text/html,application/xhtml+xml",
        "Accept-Language": "en-US,en;q=0.8",
    }


LINKEDIN_LINK_RE = re.compile(
    r"^https://(?:[a-z][a-z0-9-]*\.)?linkedin\.com/(?:in|company|school|pulse|posts|showcase)/[A-Za-z0-9._%-]+/?$"
)
PERSON_PATH_RE = re.compile(r"^/(?:in|company|school|pulse|posts|showcase)/[A-Za-z0-9._%-]+$")


def canonical_url(url: str) -> str | None:
    try:
        p = urlsplit(url.strip())
    except ValueError:
        return None
    if p.scheme not in ("http", "https") or "linkedin.com" not in p.netloc.lower():
        return None
    raw_path = p.path.rstrip("/")
    parts = raw_path.split("/")
    if len(parts) != 3 or parts[0] != "" or parts[1] not in {"in", "company", "school", "pulse", "posts", "showcase"} or not parts[2]:
        return None
    slug = quote(unicodedata.normalize("NFC", unquote(parts[2])), safe="._~-")
    path = f"/{parts[1]}/{slug}"
    if not LINKEDIN_LINK_RE.match(f"https://www.linkedin.com{path}/"):
        if not PERSON_PATH_RE.match(path):
            return None
    host = p.netloc.lower().split(":")[0]
    if host != "www.linkedin.com" and not re.match(r"^(www|[a-z]{2,5})\.linkedin\.com$", host):
        return None
    return urlunsplit(("https", "www.linkedin.com", path, "", ""))


def extract_links(base_url: str, soup, cap: int) -> list[str]:
    out, seen = [], set()
    for a in soup.find_all("a", href=True):
        full = urljoin(base_url, a["href"]).split("#")[0].split("?")[0]
        c = canonical_url(full)
        if c and c not in seen and c != base_url:
            seen.add(c)
            out.append(c)
            if len(out) >= cap:
                break
    return out


def page_type(url: str) -> str:
    if "/in/" in url:
        return "person"
    if "/company/" in url:
        return "company"
    if "/school/" in url:
        return "school"
    if "/pulse/" in url or "/posts/" in url:
        return "post"
    return "unknown"


def parse_page(url: str, html: str, ua_used: str, attempts: int) -> dict:
    soup = BeautifulSoup(html, "lxml")
    title = (soup.title.string or "").strip() if soup.title and soup.title.string else ""
    m = soup.find("meta", attrs={"name": "description"})
    meta_desc = m["content"].strip() if m and m.get("content") else ""
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
    text = soup.get_text(" ", strip=True)
    try:
        from markdownify import markdownify as md

        markdown = md(str(soup), heading_style="ATX")
    except Exception:
        markdown = text

    links = extract_links(url, soup, 64)
    rec = {
        "url": url,
        "status": 200,
        "fetched": True,
        "ua": ua_used,
        "attempts": attempts,
        "title": title,
        "type": page_type(url),
        "has_ld": int(len(ld_jsons) > 0),
        "has_person": int(any(isinstance(j, dict) and "Person" in json.dumps(j) for j in ld_jsons)),
        "h1s": h1s,
        "h2s": h2s,
        "description": meta_desc,
        "html": html,
        "text": text,
        "markdown": markdown,
        "ldJson": ld_jsons,
        "text_len": len(text),
        "linksFound": len(links),
        "links": links,
    }
    return rec


def fail_record(url: str, status, attempts: int, uas_tried: list[str]) -> dict:
    return {
        "url": url,
        "status": status,
        "fetched": False,
        "ua": ",".join(uas_tried),
        "attempts": attempts,
        "title": "",
        "type": page_type(url),
        "has_ld": 0,
        "has_person": 0,
        "h1s": [],
        "h2s": [],
        "description": "",
        "text": "",
        "markdown": "",
        "ldJson": [],
        "text_len": 0,
        "linksFound": 0,
        "links": [],
    }


def claim_frontier(
    frontier: list[tuple[str, int]],
    stats: dict[str, int | float],
    *,
    max_requests: int,
) -> tuple[str, int] | None:
    """Claim one real crawl request without counting idle workers as requests."""
    if not frontier or int(stats.get("requested", 0)) >= max_requests:
        return None
    item = frontier.pop(0)
    stats["requested"] = int(stats.get("requested", 0)) + 1
    return item


async def main() -> None:
    async with Actor:
        inp = await Actor.get_input() or {}
        seeds = [
            u.get("url")
            for u in inp.get("startUrls", [{"url": "https://www.linkedin.com/in/williamhgates"}])
            if u.get("url")
        ]
        max_requests = int(inp.get("maxRequests", len(seeds)))
        max_depth = int(inp.get("maxDepth", 0))
        max_links = int(inp.get("maxLinksPerPage", 64))
        use_proxy = bool(inp.get("useApifyProxy", True))
        proxy_groups = [g for g in (inp.get("proxyGroups", ["auto"]) or []) if g]
        conc = int(inp.get("maxConcurrency", 4))
        attempts_per_url = int(inp.get("attemptsPerUrl", 6))  # one per UA in the matrix
        inter_delay = float(inp.get("interRequestDelay", 0.0))  # Agent A pacing experiment

        Actor.log.info(
            f"searxflare UA matrix x{attempts_per_url} | proxy={use_proxy}{proxy_groups} "
            f"maxReq={max_requests} depth={max_depth} conc={conc} seeds={len(seeds)}"
        )

        proxy_conf = None
        if use_proxy:
            proxy_conf = await Actor.create_proxy_configuration(groups=proxy_groups or ["auto"])

        timeout = httpx.Timeout(30.0, connect=15.0)

        seen: set[str] = set()
        frontier: list[tuple[str, int]] = []
        for s in seeds:
            c = canonical_url(s) or s
            if c not in seen:
                seen.add(c)
                frontier.append((c, 0))

        stats = {"requested": 0, "ok200": 0, "fail": 0, "err999": 0, "err429": 0, "other": 0}
        lock = asyncio.Lock()
        sem = asyncio.Semaphore(conc)

        async def crawl_one(url: str, depth: int) -> None:
            """Try the full UA matrix until 200. ALWAYS pushes a record."""
            uas_tried: list[str] = []
            last_status: int | None = None
            for attempt in range(attempts_per_url):
                ua_key, ua_val = UA_MATRIX[attempt % len(UA_MATRIX)]
                uas_tried.append(ua_key)
                proxy_url = await proxy_conf.new_url() if proxy_conf else None
                status: int | None = None
                html = ""
                try:
                    async with httpx.AsyncClient(proxy=proxy_url, timeout=timeout) as cli:
                        r = await cli.get(url, headers=request_headers(ua_val), follow_redirects=True)
                        status, html = r.status_code, r.text
                        Actor.log.info(f"ATTEMPT {url} [{ua_key}] -> {status} bytes={len(html)}")
                except Exception as e:
                    Actor.log.warning(f"FETCH_ERR {url} [{ua_key}]: {str(e)[:80]}")
                decision = retry_decision(status, attempt)
                if decision is RetryDecision.SUCCESS:
                    stats["ok200"] += 1
                    rec = parse_page(url, html, ua_key, attempt + 1)
                    rec["depth"] = depth
                    links = rec["links"][:max_links]
                    rec["links"] = links
                    rec["linksFound"] = len(links)
                    await Actor.push_data(rec)
                    if depth < max_depth and links:
                        async with lock:
                            for l in links:
                                if l not in seen:
                                    seen.add(l)
                                    frontier.append((l, depth + 1))
                    return
                last_status = status
                if decision is RetryDecision.TERMINAL:
                    break
                if status == 429 or status is None or (status is not None and status >= 500):
                    await asyncio.sleep(2 ** min(attempt, 4) * random.uniform(1.5, 3.5))
            stats["fail"] += 1
            if last_status == 999:
                stats["err999"] += 1
            elif last_status == 429:
                stats["err429"] += 1
            else:
                stats["other"] += 1
            await Actor.push_data(fail_record(url, last_status, len(uas_tried), uas_tried))

        async def worker() -> None:
            while True:
                async with lock:
                    item = claim_frontier(frontier, stats, max_requests=max_requests)
                    if item is None:
                        return
                url, depth = item
                async with sem:
                    await asyncio.sleep(inter_delay + random.uniform(0.5, 1.5))
                    await crawl_one(url, depth)

        t0 = time.time()
        workers = [asyncio.create_task(worker()) for _ in range(conc)]
        await asyncio.gather(*workers)

        stats["wallSecs"] = round(time.time() - t0, 1)
        stats["uniqueSeen"] = len(seen)
        stats["coveragePct"] = round(100 * (stats["ok200"] + stats["fail"]) / max(stats["requested"], 1), 1)
        await Actor.set_value("CRAWL_STATS", stats)
        Actor.log.info(f"DONE {json.dumps(stats)}")
