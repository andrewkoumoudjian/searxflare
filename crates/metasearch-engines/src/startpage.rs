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

pub struct StartpageEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "startpage-web",
    display_name: "Startpage Web",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &["www.startpage.com"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: true,
        country: false,
        safe_search: true,
        time_range: true,
    },
    timeout_ms: HTML_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 0.9,
    parser_version: "startpage-web-html-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::DurableCoordinator,
    cache_policy: CachePolicy {
        response_ttl_seconds: 120,
        negative_ttl_seconds: 60,
    },
    bot_auth_policy: BotAuthPolicy::Optional,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results"),
    item: ".w-gl__result, .result",
    title: ".w-gl__result-title, .result-title",
    url: "a.w-gl__result-title, a.result-link",
    description: Some(".w-gl__description, .result-description"),
    thumbnail: None,
};

fn date_filter(value: TimeRange) -> &'static str {
    match value {
        TimeRange::Day => "d",
        TimeRange::Week => "w",
        TimeRange::Month => "m",
        TimeRange::Year => "y",
    }
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.page_number() > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Startpage paging is capped at ten pages",
        ));
    }
    let url = Url::parse("https://www.startpage.com/sp/search").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    let page = query.page_number().to_string();
    let safe = match query.safe_search {
        SafeSearch::Off => "0",
        SafeSearch::Moderate => "1",
        SafeSearch::Strict => "2",
    };
    let mut form = vec![
        ("query", query.text.as_str()),
        ("page", page.as_str()),
        ("cat", "web"),
        ("sc", safe),
    ];
    let date = query.time_range.map(date_filter);
    if let Some(date) = date {
        form.push(("with_date", date));
    }
    let body = serde_urlencoded::to_string(form)
        .map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?
        .into_bytes();
    Ok(EngineRequest {
        method: EngineMethod::Post,
        url,
        headers: BTreeMap::from([
            (
                "accept".into(),
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".into(),
            ),
            (
                "content-type".into(),
                "application/x-www-form-urlencoded".into(),
            ),
            (
                "user-agent".into(),
                "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: Some(body),
        accepted_content_types: &["text/html", "application/xhtml+xml"],
    })
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for StartpageEngine {
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
                "Startpage is in a shared provider cooldown",
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
                thumbnail: None,
                category: "general".into(),
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
    fn builds_bounded_post_request() {
        let query = NormalizedQuery {
            text: "privacy search".into(),
            page: Some(3),
            limit: 10,
            locale: Some("en-US".into()),
            country: None,
            categories: vec!["general".into()],
            engines: vec![],
            safe_search: SafeSearch::Moderate,
            time_range: Some(TimeRange::Month),
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
            cursor: None,
        };
        let request = build_request(&query).unwrap();
        let body = String::from_utf8(request.body.unwrap()).unwrap();
        assert!(body.contains("query=privacy+search"));
        assert!(body.contains("page=3"));
        assert!(body.contains("with_date=m"));
    }
}
