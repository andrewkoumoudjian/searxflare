use metasearch_core::FailureKind;
use metasearch_http::classify_challenge;

#[test]
fn fixture_challenges_are_classified() {
    for (engine, body) in [
        ("arxiv", include_bytes!("../../../fixtures/engines/arxiv/challenge.html").as_slice()),
        ("wikipedia", include_bytes!("../../../fixtures/engines/wikipedia/challenge.html").as_slice()),
        ("duckduckgo-html", include_bytes!("../../../fixtures/engines/duckduckgo-html/challenge.html").as_slice()),
    ] {
        assert_eq!(classify_challenge(engine, 403, body).unwrap().kind, FailureKind::EngineChallenged);
    }
}

#[test]
fn fixture_access_denial_and_rate_limit_are_classified() {
    assert_eq!(
        classify_challenge("wikipedia", 403, include_bytes!("../../../fixtures/engines/wikipedia/access-denied.html")).unwrap().kind,
        FailureKind::EngineAccessDenied
    );
    assert_eq!(
        classify_challenge("arxiv", 429, include_bytes!("../../../fixtures/engines/arxiv/rate-limited.html")).unwrap().kind,
        FailureKind::EngineRateLimited
    );
}
