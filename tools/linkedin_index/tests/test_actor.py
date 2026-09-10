from __future__ import annotations

import sys
from pathlib import Path


ACTOR_ROOT = Path(__file__).resolve().parents[1] / "actor"
sys.path.insert(0, str(ACTOR_ROOT))

from my_actor import main as actor_main  # noqa: E402


def test_parse_page_retains_complete_successful_ssr_content():
    long_text = "profile-segment " * 80_000
    html = (
        "<html><head><title>Example</title>"
        '<script type="application/ld+json">{"@type":"Person","name":"Example"}</script>'
        "</head><body><h1>Example Person</h1><p>"
        + long_text
        + "</p><p>FINAL_UNTRUNCATED_MARKER</p></body></html>"
    )

    record = actor_main.parse_page(
        "https://www.linkedin.com/in/example",
        html,
        "claude",
        1,
    )

    assert record["html"] == html
    assert "FINAL_UNTRUNCATED_MARKER" in record["text"]
    assert "FINAL\\_UNTRUNCATED\\_MARKER" in record["markdown"]
    assert record["has_person"] == 1


def test_parse_page_emits_canonical_discovered_link_urls():
    html = """
    <html><body>
      <a href="https://ca.linkedin.com/company/acme/?trk=profile">Acme</a>
      <a href="/in/other-person/?miniProfileUrn=abc">Other</a>
      <a href="https://example.com/not-linkedin">Ignore</a>
    </body></html>
    """
    record = actor_main.parse_page(
        "https://www.linkedin.com/in/example",
        html,
        "claude",
        1,
    )

    assert record["links"] == [
        "https://www.linkedin.com/company/acme",
        "https://www.linkedin.com/in/other-person",
    ]
    assert record["linksFound"] == 2


def test_retry_decision_stops_on_deterministic_999_and_retries_transients():
    assert actor_main.retry_decision(999, 0) is actor_main.RetryDecision.TERMINAL
    assert actor_main.retry_decision(404, 0) is actor_main.RetryDecision.TERMINAL
    assert actor_main.retry_decision(429, 0) is actor_main.RetryDecision.RETRY
    assert actor_main.retry_decision(503, 0) is actor_main.RetryDecision.RETRY
    assert actor_main.retry_decision(None, 0) is actor_main.RetryDecision.RETRY
    assert actor_main.retry_decision(200, 0) is actor_main.RetryDecision.SUCCESS


def test_actor_canonical_url_normalizes_unicode_percent_encoding():
    assert actor_main.canonical_url(
        "https://www.linkedin.com/in/s%c3%a9rgio-faustino-0298981"
    ) == "https://www.linkedin.com/in/s%C3%A9rgio-faustino-0298981"
    assert actor_main.canonical_url(
        "https://www.linkedin.com/in/sérgio-faustino-0298981"
    ) == "https://www.linkedin.com/in/s%C3%A9rgio-faustino-0298981"


def test_claim_frontier_counts_only_real_requests_and_respects_limit():
    frontier = [
        ("https://www.linkedin.com/in/a", 0),
        ("https://www.linkedin.com/in/b", 0),
    ]
    stats = {"requested": 0}

    assert actor_main.claim_frontier(frontier, stats, max_requests=5) == (
        "https://www.linkedin.com/in/a",
        0,
    )
    assert actor_main.claim_frontier(frontier, stats, max_requests=5) == (
        "https://www.linkedin.com/in/b",
        0,
    )
    assert actor_main.claim_frontier(frontier, stats, max_requests=5) is None
    assert stats["requested"] == 2

    frontier = [
        ("https://www.linkedin.com/in/c", 0),
        ("https://www.linkedin.com/in/d", 0),
    ]
    stats = {"requested": 1}
    assert actor_main.claim_frontier(frontier, stats, max_requests=1) is None
    assert stats["requested"] == 1
    assert len(frontier) == 2
