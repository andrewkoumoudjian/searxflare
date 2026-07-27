use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderCoordinatorCommand, ProviderResult, SafeSearch, SearchEngine, SourceKind, StatePolicy,
    TimeRange, DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
    HTML_ENGINE_TIMEOUT_MS,
};
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use serde_json::Map;
use std::collections::BTreeMap;
use url::Url;

pub struct GoogleEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "google-web",
    display_name: "Google Web",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &["www.google.com"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: true,
        country: true,
        safe_search: true,
        time_range: true,
    },
    timeout_ms: HTML_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.0,
    parser_version: "google-web-html-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::DurableCoordinator,
    cache_policy: CachePolicy {
        response_ttl_seconds: 120,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Optional,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some("#topstuff .card-section, .med .card-section"),
    item: "div.MjjYud, div.ezO2md",
    title: "h3",
    url: "a",
    description: Some("div.VwiC3b, div.FrIlee, span.aCOpRe"),
    thumbnail: None,
};

fn locale_parts(query: &NormalizedQuery) -> (String, String) {
    let mut parts = query.locale.as_deref().unwrap_or("en-US").split(['-', '_']);
    let language = parts.next().unwrap_or("en").to_ascii_lowercase();
    let country = query
        .country
        .as_deref()
        .or_else(|| parts.next())
        .unwrap_or("US")
        .to_ascii_uppercase();
    (language, country)
}

fn time_range_code(value: TimeRange) -> &'static str {
    match value {
        TimeRange::Day => "qdr:d",
        TimeRange::Week => "qdr:w",
        TimeRange::Month => "qdr:m",
        TimeRange::Year => "qdr:y",
    }
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Google web supports at most ten pages",
        ));
    }

    let (language, country) = locale_parts(query);
    let count = u32::from(query.limit.min(10));
    let start = page.saturating_sub(1).saturating_mul(10);
    let mut url = Url::parse("https://www.google.com/search").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("q", &query.text)
            .append_pair("start", &start.to_string())
            .append_pair("num", &count.to_string())
            .append_pair("hl", &language)
            .append_pair("gl", &country)
            .append_pair(
                "safe",
                if query.safe_search == SafeSearch::Off {
                    "off"
                } else {
                    "active"
                },
            )
            .append_pair("filter", "0");
        if let Some(time_range) = query.time_range {
            pairs.append_pair("tbs", time_range_code(time_range));
        }
    }

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            (
                "accept".into(),
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".into(),
            ),
            (
                "accept-language".into(),
                query
                    .locale
                    .clone()
                    .unwrap_or_else(|| "en-US,en;q=0.8".into()),
            ),
            (
                "user-agent".into(),
                "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["text/html", "application/xhtml+xml"],
    })
}

fn unwrap_google_url(value: &str) -> String {
    let Ok(url) = Url::parse(value) else {
        return value.to_owned();
    };
    if url.host_str() == Some("www.google.com") && url.path() == "/url" {
        if let Some((_, target)) = url
            .query_pairs()
            .find(|(key, _)| key == "q" || key == "url")
        {
            if Url::parse(&target).is_ok() {
                return target.into_owned();
            }
        }
    }
    value.to_owned()
}

fn parse_results(body: &[u8], request_url: &Url) -> Result<Vec<ProviderResult>, EngineFailure> {
    let parsed = parse_selector_results(body, request_url, &SELECTORS, 20).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    Ok(parsed
        .into_iter()
        .enumerate()
        .map(|(index, result)| ProviderResult {
            url: unwrap_google_url(&result.url),
            title: result.title,
            content: result.description.unwrap_or_default(),
            published_at: None,
            thumbnail: None,
            category: "general".into(),
            metadata: Map::new(),
            engine_id: DESCRIPTOR.id.into(),
            position: (index + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        })
        .collect())
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for GoogleEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let snapshot = context
            .coordinator
            .execute(
                DESCRIPTOR.id,
                ProviderCoordinatorCommand::Snapshot {
                    now_ms: context.now_ms,
                },
            )
            .await?;
        if snapshot
            .cooldown_expires_at_ms
            .is_some_and(|expiry| expiry > context.now_ms)
        {
            return Err(EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineRateLimited,
                "Google is in a shared provider cooldown",
            ));
        }
        let request = build_request(query)?;
        let request_url = request.url.clone();
        let response = match context
            .http
            .send(&DESCRIPTOR, request, context.deadline)
            .await
        {
            Ok(response) => response,
            Err(failure) => {
                if matches!(
                    failure.kind,
                    FailureKind::EngineRateLimited
                        | FailureKind::EngineChallenged
                        | FailureKind::EngineAccessDenied
                ) {
                    let _ = context
                        .coordinator
                        .execute(
                            DESCRIPTOR.id,
                            ProviderCoordinatorCommand::RecordFailure {
                                now_ms: context.now_ms,
                                failure_kind: failure.kind.as_code().into(),
                                cooldown_ms: 300_000,
                            },
                        )
                        .await;
                }
                return Err(failure);
            }
        };
        let results = parse_results(&response.body, &request_url)?;
        let _ = context
            .coordinator
            .execute(
                DESCRIPTOR.id,
                ProviderCoordinatorCommand::RecordSuccess {
                    now_ms: context.now_ms,
                },
            )
            .await;
        Ok(EngineOutput {
            results,
            next_cursor: None,
            upstream_requests: 1,
            response_bytes: response.body.len(),
            parse_ms: 0,
            redirect_count: response.redirect_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metasearch_core::RankingStrategy;

    fn query() -> NormalizedQuery {
        NormalizedQuery {
            text: "cloudflare rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 20,
            locale: Some("fr-CA".into()),
            country: None,
            safe_search: SafeSearch::Strict,
            time_range: Some(TimeRange::Month),
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_locale_safe_and_time_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("www.google.com"));
        assert_eq!(parameters.get("start").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("num").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("hl").map(String::as_str), Some("fr"));
        assert_eq!(parameters.get("gl").map(String::as_str), Some("CA"));
        assert_eq!(parameters.get("safe").map(String::as_str), Some("active"));
        assert_eq!(parameters.get("tbs").map(String::as_str), Some("qdr:m"));
    }

    #[test]
    fn rejects_excessive_pages() {
        let mut value = query();
        value.page = Some(11);
        assert_eq!(
            build_request(&value).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn unwraps_google_tracking_urls() {
        assert_eq!(
            unwrap_google_url("https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fitem&sa=U"),
            "https://example.com/item"
        );
        assert_eq!(
            unwrap_google_url("https://example.com/direct"),
            "https://example.com/direct"
        );
    }

    #[test]
    fn parses_normal_empty_and_changed_layout_fixtures() {
        let url = Url::parse("https://www.google.com/search?q=cloudflare").unwrap();
        let normal = parse_results(
            include_bytes!("../../../fixtures/engines/google-web/normal.html"),
            &url,
        )
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Google result");
        assert_eq!(normal[0].url, "https://example.com/google");

        let empty = parse_results(
            include_bytes!("../../../fixtures/engines/google-web/empty.html"),
            &url,
        )
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(
            include_bytes!("../../../fixtures/engines/google-web/changed-layout.html"),
            &url,
        )
        .is_err());
    }
}
