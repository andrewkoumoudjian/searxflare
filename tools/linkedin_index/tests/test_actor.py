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


def test_retry_decision_stops_on_deterministic_999_and_retries_transients():
    assert actor_main.retry_decision(999, 0) is actor_main.RetryDecision.TERMINAL
    assert actor_main.retry_decision(404, 0) is actor_main.RetryDecision.TERMINAL
    assert actor_main.retry_decision(429, 0) is actor_main.RetryDecision.RETRY
    assert actor_main.retry_decision(503, 0) is actor_main.RetryDecision.RETRY
    assert actor_main.retry_decision(None, 0) is actor_main.RetryDecision.RETRY
    assert actor_main.retry_decision(200, 0) is actor_main.RetryDecision.SUCCESS
