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

pub struct BraveNewsEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "brave-news",
    display_name: "Brave News",
    categories: &["news"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["search.brave.com"],
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
    parser_version: "brave-news-html-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::DurableCoordinator,
    cache_policy: CachePolicy {
        response_ttl_seconds: 120,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Optional,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results"),
    item: ".snippet[data-type=\"news\"], .news-result",
    title: ".title, .snippet-title",
    url: "a",
    description: Some(".description, .content"),
    thumbnail: Some("img"),
};

fn time_range_code(value: TimeRange) -> &'static str {
    match value {
        TimeRange::Day => "pd",
        TimeRange::Week => "pw",
        TimeRange::Month => "pm",
        TimeRange::Year => "py",
    }
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.page_number() > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Brave News paging is capped at ten pages",
        ));
    }
    let mut url = Url::parse("https://search.brave.com/news").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("q", &query.text)
            .append_pair("offset", &query.page_number().saturating_sub(1).to_string())
            .append_pair(
                "safesearch",
                match query.safe_search {
                    SafeSearch::Off => "off",
                    SafeSearch::Moderate => "moderate",
                    SafeSearch::Strict => "strict",
                },
            );
        if let Some(country) = query.country.as_deref() {
            pairs.append_pair("country", &country.to_ascii_lowercase());
        }
        if let Some(value) = query.time_range {
            pairs.append_pair("tf", time_range_code(value));
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
                "user-agent".into(),
                "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["text/html", "application/xhtml+xml"],
    })
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for BraveNewsEngine {
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
                "Brave News is in a shared provider cooldown",
            ));
        }
        let request = build_request(query)?;
        let response = match context
            .http
            .send(&DESCRIPTOR, request.clone(), context.deadline)
            .await
        {
            Ok(response) => response,
            Err(failure) => {
                if matches!(
                    failure.kind,
                    FailureKind::EngineRateLimited | FailureKind::EngineChallenged
                ) {
                    let _ = context
                        .coordinator
                        .execute(
                            DESCRIPTOR.id,
                            ProviderCoordinatorCommand::RecordFailure {
                                now_ms: context.now_ms,
                                failure_kind: failure.kind.as_code().into(),
                                cooldown_ms: 60_000,
                            },
                        )
                        .await;
                }
                return Err(failure);
            }
        };
        let parsed = parse_selector_results(&response.body, &request.url, &SELECTORS, 20).map_err(
            |error| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineParseFailed,
                    error.to_string(),
                )
            },
        )?;
        let results = parsed
            .into_iter()
            .enumerate()
            .map(|(index, result)| ProviderResult {
                url: result.url,
                title: result.title,
                content: result.description.unwrap_or_default(),
                published_at: None,
                thumbnail: result.thumbnail,
                category: "news".into(),
                metadata: Map::new(),
                engine_id: DESCRIPTOR.id.into(),
                position: (index + 1) as u32,
                engine_weight: DESCRIPTOR.weight,
            })
            .collect();
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

    #[test]
    fn builds_bounded_news_request() {
        let query = NormalizedQuery {
            text: "cloudflare".into(),
            page: Some(2),
            limit: 10,
            locale: Some("en-US".into()),
            country: Some("CA".into()),
            categories: vec!["news".into()],
            engines: vec![],
            safe_search: SafeSearch::Strict,
            time_range: Some(TimeRange::Week),
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
            cursor: None,
        };
        let request = build_request(&query).unwrap();
        assert_eq!(
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "offset")
                .unwrap()
                .1,
            "1"
        );
        assert_eq!(
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "country")
                .unwrap()
                .1,
            "ca"
        );
    }
}
