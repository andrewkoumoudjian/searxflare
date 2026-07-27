use crate::text::strip_markup;
use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderCoordinatorCommand, ProviderResult, SafeSearch, SearchEngine, SourceKind, StatePolicy,
    TimeRange, DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
    HTML_ENGINE_TIMEOUT_MS,
};
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use serde_json::{Map, Value};
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
    parser_version: "startpage-web-react-v2",
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

const SC_CODE_STATE_KEY: &str = "sc-code";
const REACT_MARKER: &str = "React.createElement(UIStartpage.AppSerpWeb, ";

fn date_filter(value: TimeRange) -> &'static str {
    match value {
        TimeRange::Day => "d",
        TimeRange::Week => "w",
        TimeRange::Month => "m",
        TimeRange::Year => "y",
    }
}

fn safe_search_code(value: SafeSearch) -> &'static str {
    match value {
        SafeSearch::Off => "none",
        SafeSearch::Moderate => "moderate",
        SafeSearch::Strict => "heavy",
    }
}

fn build_home_request() -> Result<EngineRequest, EngineFailure> {
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url: Url::parse("https://www.startpage.com/").map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?,
        headers: BTreeMap::from([
            ("accept".into(), "text/html,application/xhtml+xml".into()),
            ("accept-language".into(), "en-US,en;q=0.8".into()),
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

fn extract_sc_code(body: &[u8]) -> Result<String, EngineFailure> {
    let html = String::from_utf8_lossy(body);
    let name = html.find("name=\"sc\"").or_else(|| html.find("name='sc'"));
    let position = name.ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineChallenged,
            "Startpage homepage did not expose an sc search token",
        )
    })?;
    let start = html[..position].rfind("<input").unwrap_or(position);
    let end = html[position..]
        .find('>')
        .map(|offset| position + offset + 1)
        .unwrap_or_else(|| html.len().min(position + 1024));
    let input = &html[start..end];
    for quote in ['"', '\''] {
        let marker = format!("value={quote}");
        if let Some(value_start) = input.find(&marker) {
            let value_start = value_start + marker.len();
            if let Some(value) = input[value_start..].split(quote).next() {
                if !value.is_empty() && value.len() <= 512 {
                    return Ok(value.into());
                }
            }
        }
    }
    Err(EngineFailure::new(
        DESCRIPTOR.id,
        FailureKind::EngineParseFailed,
        "Startpage sc search token has no value",
    ))
}

fn build_request(query: &NormalizedQuery, sc_code: &str) -> Result<EngineRequest, EngineFailure> {
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
    let safe = safe_search_code(query.safe_search);
    let mut form = vec![
        ("query", query.text.as_str()),
        ("cat", "web"),
        ("t", "device"),
        ("sc", sc_code),
        ("abd", "1"),
        ("abe", "1"),
        ("qsr", "all"),
        ("qadf", safe),
        ("language", "en"),
        ("lui", "en"),
    ];
    if query.page_number() > 1 {
        form.push(("page", page.as_str()));
        form.push(("segment", "startpage.udog"));
    }
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
            ("origin".into(), "https://www.startpage.com".into()),
            ("referer".into(), "https://www.startpage.com/".into()),
        ]),
        cookies: BTreeMap::new(),
        body: Some(body),
        accepted_content_types: &["text/html", "application/xhtml+xml"],
    })
}

fn react_props(body: &[u8]) -> Result<Option<Value>, EngineFailure> {
    let html = String::from_utf8_lossy(body);
    let Some(marker_start) = html.find(REACT_MARKER) else {
        return Ok(None);
    };
    let start = marker_start + REACT_MARKER.len();
    let bytes = html.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for index in start..bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return serde_json::from_str(&html[start..=index])
                        .map(Some)
                        .map_err(|error| {
                            EngineFailure::new(
                                DESCRIPTOR.id,
                                FailureKind::EngineParseFailed,
                                error.to_string(),
                            )
                        });
                }
            }
            _ => {}
        }
    }
    Err(EngineFailure::new(
        DESCRIPTOR.id,
        FailureKind::EngineParseFailed,
        "Startpage React result payload was truncated",
    ))
}

fn provider_result(url: String, title: String, content: String, position: usize) -> ProviderResult {
    ProviderResult {
        url,
        title,
        content,
        published_at: None,
        thumbnail: None,
        category: "general".into(),
        metadata: Map::new(),
        engine_id: DESCRIPTOR.id.into(),
        position: position as u32,
        engine_weight: DESCRIPTOR.weight,
    }
}

fn parse_results(body: &[u8], request_url: &Url) -> Result<Vec<ProviderResult>, EngineFailure> {
    if let Some(root) = react_props(body)? {
        let mainline = root
            .pointer("/render/presenter/regions/mainline")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineParseFailed,
                    "Startpage React payload is missing mainline regions",
                )
            })?;
        let mut results = Vec::new();
        for group in mainline {
            if group.get("display_type").and_then(Value::as_str) != Some("web-google") {
                continue;
            }
            for item in group
                .get("results")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(url) = item
                    .get("clickUrl")
                    .and_then(Value::as_str)
                    .filter(|url| Url::parse(url).is_ok())
                else {
                    continue;
                };
                let Some(title) = item
                    .get("title")
                    .and_then(Value::as_str)
                    .map(strip_markup)
                    .filter(|title| !title.is_empty())
                else {
                    continue;
                };
                let content = item
                    .get("description")
                    .and_then(Value::as_str)
                    .map(strip_markup)
                    .unwrap_or_default();
                results.push(provider_result(
                    url.into(),
                    title,
                    content,
                    results.len() + 1,
                ));
                if results.len() == 20 {
                    break;
                }
            }
        }
        return Ok(results);
    }
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
        .map(|(index, result)| {
            provider_result(
                result.url,
                result.title,
                result.description.unwrap_or_default(),
                index + 1,
            )
        })
        .collect())
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
        let mut upstream_requests = 0u8;
        let mut response_bytes = 0usize;
        let mut redirect_count = 0u8;
        let sc_code = if let Some(value) = context
            .state
            .get(DESCRIPTOR.id, SC_CODE_STATE_KEY)
            .await?
            .and_then(|value| String::from_utf8(value).ok())
        {
            value
        } else {
            let homepage = context
                .http
                .send(&DESCRIPTOR, build_home_request()?, context.deadline)
                .await?;
            upstream_requests = upstream_requests.saturating_add(1);
            response_bytes = response_bytes.saturating_add(homepage.body.len());
            redirect_count = redirect_count.saturating_add(homepage.redirect_count);
            let value = extract_sc_code(&homepage.body)?;
            context
                .state
                .put(
                    DESCRIPTOR.id,
                    SC_CODE_STATE_KEY,
                    value.as_bytes(),
                    Some(3_600),
                )
                .await?;
            value
        };
        let request = build_request(query, &sc_code)?;
        let request_url = request.url.clone();
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
        upstream_requests = upstream_requests.saturating_add(1);
        response_bytes = response_bytes.saturating_add(response.body.len());
        redirect_count = redirect_count.saturating_add(response.redirect_count);
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
            upstream_requests,
            response_bytes,
            parse_ms: 0,
            redirect_count,
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
        let request = build_request(&query, "test-sc").unwrap();
        let body = String::from_utf8(request.body.unwrap()).unwrap();
        assert!(body.contains("query=privacy+search"));
        assert!(body.contains("page=3"));
        assert!(body.contains("with_date=m"));
        assert!(body.contains("sc=test-sc"));
        assert!(body.contains("qadf=moderate"));
    }

    #[test]
    fn parses_homepage_sc_and_react_results() {
        assert_eq!(
            extract_sc_code(br#"<form><input name="sc" value="sc-value"></form>"#).unwrap(),
            "sc-value"
        );
        let html = br#"<script>React.createElement(UIStartpage.AppSerpWeb, {"render":{"presenter":{"regions":{"mainline":[{"display_type":"web-google","results":[{"title":"<b>Privacy</b> result","clickUrl":"https://example.com/privacy","description":"Private &amp; useful"}]}]}}}});</script>"#;
        let results = parse_results(
            html,
            &Url::parse("https://www.startpage.com/sp/search").unwrap(),
        )
        .unwrap();
        assert_eq!(results[0].title, "Privacy result");
        assert_eq!(results[0].content, "Private & useful");
    }
}
