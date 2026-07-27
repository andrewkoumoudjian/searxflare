use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct SemanticScholarEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "semantic-scholar",
    display_name: "Semantic Scholar",
    categories: &["academic"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["www.semanticscholar.org"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: false,
        country: false,
        safe_search: false,
        time_range: false,
    },
    timeout_ms: DEFAULT_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.35,
    parser_version: "semantic-scholar-web-search-json-v2",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::KvSnapshot,
    cache_policy: CachePolicy {
        response_ttl_seconds: 900,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

const UI_VERSION_STATE_KEY: &str = "s2-ui-version";

fn build_home_request() -> Result<EngineRequest, EngineFailure> {
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url: Url::parse("https://www.semanticscholar.org").map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?,
        headers: BTreeMap::from([
            ("accept".into(), "text/html,application/xhtml+xml".into()),
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

fn html_attribute(fragment: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let marker = format!("{name}={quote}");
        let Some(start) = fragment.find(&marker) else {
            continue;
        };
        let start = start + marker.len();
        let value = fragment[start..].split(quote).next()?.trim();
        if !value.is_empty() && value.len() <= 128 {
            return Some(value.into());
        }
    }
    None
}

fn extract_ui_version(body: &[u8]) -> Result<String, EngineFailure> {
    let html = String::from_utf8_lossy(body);
    let marker = "s2-ui-version";
    let position = html.find(marker).ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            "Semantic Scholar homepage is missing s2-ui-version",
        )
    })?;
    let start = html[..position].rfind("<meta").unwrap_or(position);
    let end = html[position..]
        .find('>')
        .map(|offset| position + offset + 1)
        .unwrap_or_else(|| html.len().min(position + 512));
    html_attribute(&html[start..end], "content").ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            "Semantic Scholar s2-ui-version has no content value",
        )
    })
}

fn build_request(
    query: &NormalizedQuery,
    ui_version: &str,
) -> Result<EngineRequest, EngineFailure> {
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Semantic Scholar time-range filtering is not exposed by this adapter",
        ));
    }

    if query.page_number() > 100 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Semantic Scholar relevance search exposes at most 100 pages",
        ));
    }

    let url = Url::parse("https://www.semanticscholar.org/api/1/search").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    let body = serde_json::to_vec(&json!({
        "queryString": query.text,
        "page": query.page_number(),
        "pageSize": query.limit,
        "sort": "relevance",
        "getQuerySuggestions": false,
        "authors": [],
        "coAuthors": [],
        "venues": [],
        "performTitleMatch": true
    }))
    .map_err(|error| EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string()))?;
    Ok(EngineRequest {
        method: EngineMethod::Post,
        url,
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("content-type".into(), "application/json".into()),
            ("x-s2-client".into(), "webapp-browser".into()),
            ("x-s2-ui-version".into(), ui_version.into()),
            (
                "user-agent".into(),
                "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: Some(body),
        accepted_content_types: &["application/json", "text/json"],
    })
}

fn classify_api_error(root: &Value) -> EngineFailure {
    let message = root
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| root.get("message").and_then(Value::as_str))
        .unwrap_or("Semantic Scholar returned an unsuccessful API response");
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("rate") || lower.contains("too many") {
        FailureKind::EngineRateLimited
    } else if lower.contains("api key") || lower.contains("unauthorized") {
        FailureKind::EngineAccessDenied
    } else {
        FailureKind::EngineParseFailed
    };
    EngineFailure::new(DESCRIPTOR.id, kind, message)
}

fn authors(item: &Value) -> Vec<String> {
    item.get("authors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|author| {
            author.get("name").and_then(Value::as_str).or_else(|| {
                author
                    .as_array()
                    .and_then(|parts| parts.first())
                    .and_then(|author| author.get("name"))
                    .and_then(Value::as_str)
            })
        })
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn result_url(item: &Value) -> Option<String> {
    let direct = item
        .pointer("/primaryPaperLink/url")
        .and_then(Value::as_str)
        .or_else(|| item.get("url").and_then(Value::as_str))
        .or_else(|| {
            item.get("links")
                .and_then(Value::as_array)
                .and_then(|links| links.first())
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let candidate = direct.or_else(|| {
        item.get("paperId")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|paper_id| format!("https://www.semanticscholar.org/paper/{paper_id}"))
    })?;
    Url::parse(&candidate)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|_| candidate)
}

fn parse_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if root.get("error").is_some()
        || root.get("message").is_some()
            && root.get("data").is_none()
            && root.get("results").is_none()
    {
        return Err(classify_api_error(&root));
    }
    let items = root
        .get("data")
        .or_else(|| root.get("results"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Semantic Scholar response is missing data or results",
            )
        })?;

    let mut results = Vec::new();
    for item in items {
        let title = item
            .get("title")
            .and_then(|title| {
                title
                    .as_str()
                    .or_else(|| title.get("text").and_then(Value::as_str))
            })
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(title) = title else {
            continue;
        };
        let Some(url) = result_url(item) else {
            continue;
        };

        let mut metadata = Map::new();
        metadata.insert("authors".into(), json!(authors(item)));
        for field in [
            "paperId",
            "venue",
            "externalIds",
            "citationCount",
            "influentialCitationCount",
            "isOpenAccess",
            "openAccessPdf",
            "fieldsOfStudy",
            "publicationTypes",
            "journal",
        ] {
            if let Some(value) = item.get(field).filter(|value| !value.is_null()) {
                metadata.insert(field.into(), value.clone());
            }
        }
        if let Some(year) = item.get("year").and_then(Value::as_i64) {
            metadata.insert("year".into(), json!(year));
        }

        results.push(ProviderResult {
            url,
            title: title.into(),
            content: item
                .get("abstract")
                .and_then(Value::as_str)
                .or_else(|| item.pointer("/paperAbstract/text").and_then(Value::as_str))
                .unwrap_or_default()
                .trim()
                .into(),
            published_at: item
                .get("pubDate")
                .and_then(Value::as_str)
                .or_else(|| item.get("publicationDate").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    item.get("year")
                        .and_then(Value::as_i64)
                        .map(|year| year.to_string())
                }),
            thumbnail: None,
            category: "academic".into(),
            metadata,
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for SemanticScholarEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let mut upstream_requests = 0u8;
        let mut response_bytes = 0usize;
        let mut redirect_count = 0u8;
        let ui_version = if let Some(value) = context
            .state
            .get(DESCRIPTOR.id, UI_VERSION_STATE_KEY)
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
            let value = extract_ui_version(&homepage.body)?;
            context
                .state
                .put(
                    DESCRIPTOR.id,
                    UI_VERSION_STATE_KEY,
                    value.as_bytes(),
                    Some(300),
                )
                .await?;
            value
        };
        let response = context
            .http
            .send(
                &DESCRIPTOR,
                build_request(query, &ui_version)?,
                context.deadline,
            )
            .await?;
        upstream_requests = upstream_requests.saturating_add(1);
        response_bytes = response_bytes.saturating_add(response.body.len());
        redirect_count = redirect_count.saturating_add(response.redirect_count);
        let results = parse_results(&response.body)?;
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
    use metasearch_core::{RankingStrategy, SafeSearch, TimeRange};

    fn query() -> NormalizedQuery {
        NormalizedQuery {
            text: "cloudflare-rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 10,
            locale: None,
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::RrfV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_relevance_search_request() {
        let request = build_request(&query(), "ui-version").unwrap();
        assert_eq!(
            request.headers.get("x-s2-ui-version").map(String::as_str),
            Some("ui-version")
        );
        let body: Value = serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["queryString"], "cloudflare-rust");
        assert_eq!(body["page"], 2);
        assert_eq!(body["pageSize"], 10);
    }

    #[test]
    fn rejects_unsupported_time_range_and_excessive_offset() {
        let mut range_query = query();
        range_query.time_range = Some(TimeRange::Day);
        assert_eq!(
            build_request(&range_query, "ui-version").unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );

        let mut page_query = query();
        page_query.page = Some(101);
        assert_eq!(
            build_request(&page_query, "ui-version").unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn parses_normal_empty_and_failure_fixtures() {
        let normal = parse_results(include_bytes!(
            "../../../fixtures/engines/semantic-scholar/normal.json"
        ))
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Cloudflare Workers for academic search");
        assert_eq!(normal[0].published_at.as_deref(), Some("2026-07-02"));

        let empty = parse_results(include_bytes!(
            "../../../fixtures/engines/semantic-scholar/empty.json"
        ))
        .unwrap();
        assert!(empty.is_empty());

        assert_eq!(
            parse_results(include_bytes!(
                "../../../fixtures/engines/semantic-scholar/rate-limited.json"
            ))
            .unwrap_err()
            .kind,
            FailureKind::EngineRateLimited
        );
        assert!(parse_results(include_bytes!(
            "../../../fixtures/engines/semantic-scholar/changed-schema.json"
        ))
        .is_err());
    }

    #[test]
    fn extracts_ui_version_and_parses_public_web_results() {
        assert_eq!(
            extract_ui_version(
                br#"<html><head><meta name="s2-ui-version" content="abc123"></head></html>"#
            )
            .unwrap(),
            "abc123"
        );
        let results = parse_results(
            br#"{"results":[{"id":"paper-id","title":{"text":"Public web result"},"paperAbstract":{"text":"Abstract"},"pubDate":"2026-07-27","authors":[[{"name":"Ada Example"}]]}]}"#,
        )
        .unwrap();
        assert_eq!(results[0].title, "Public web result");
        assert_eq!(results[0].published_at.as_deref(), Some("2026-07-27"));
    }
}
