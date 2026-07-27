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
    allowed_hosts: &["api.semanticscholar.org"],
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
    parser_version: "semantic-scholar-paper-search-json-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 900,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

const FIELDS: &str = "title,abstract,url,authors,year,publicationDate,venue,externalIds,citationCount,influentialCitationCount,isOpenAccess,openAccessPdf,fieldsOfStudy,publicationTypes,journal";

#[cfg(test)]
fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    build_request_with_token(query, None)
}

fn build_request_with_token(
    query: &NormalizedQuery,
    token: Option<&str>,
) -> Result<EngineRequest, EngineFailure> {
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Semantic Scholar time-range filtering is not exposed by this adapter",
        ));
    }

    let limit = u32::from(query.limit);
    let offset = query.page_number().saturating_sub(1).saturating_mul(limit);
    if offset.saturating_add(limit) > 1_000 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Semantic Scholar relevance search exposes at most 1,000 results",
        ));
    }

    let mut url =
        Url::parse("https://api.semanticscholar.org/graph/v1/paper/search").map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
    url.query_pairs_mut()
        .append_pair("query", &query.text.replace('-', " "))
        .append_pair("offset", &offset.to_string())
        .append_pair("limit", &limit.to_string())
        .append_pair("fields", FIELDS);

    let mut headers = BTreeMap::from([
        ("accept".into(), "application/json".into()),
        (
            "user-agent".into(),
            "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
        ),
    ]);
    if let Some(token) = token {
        headers.insert("x-api-key".into(), token.into());
    }
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers,
        cookies: BTreeMap::new(),
        body: None,
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
        .filter_map(|author| author.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn result_url(item: &Value) -> Option<String> {
    let direct = item
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let candidate = direct.or_else(|| {
        item.get("paperId")
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
    if root.get("error").is_some() || root.get("message").is_some() && root.get("data").is_none() {
        return Err(classify_api_error(&root));
    }
    let items = root.get("data").and_then(Value::as_array).ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            "Semantic Scholar response is missing data",
        )
    })?;

    let mut results = Vec::new();
    for item in items {
        let title = item
            .get("title")
            .and_then(Value::as_str)
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
                .unwrap_or_default()
                .trim()
                .into(),
            published_at: item
                .get("publicationDate")
                .and_then(Value::as_str)
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
        let token = context.secrets.get(DESCRIPTOR.id, "api_key");
        let response = context
            .http
            .send(
                &DESCRIPTOR,
                build_request_with_token(query, token.as_deref())?,
                context.deadline,
            )
            .await?;
        let results = parse_results(&response.body)?;
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
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(
            parameters.get("query").map(String::as_str),
            Some("cloudflare rust")
        );
        assert_eq!(parameters.get("offset").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("limit").map(String::as_str), Some("10"));
        assert!(parameters
            .get("fields")
            .is_some_and(|value| value.contains("citationCount")));
    }

    #[test]
    fn rejects_unsupported_time_range_and_excessive_offset() {
        let mut range_query = query();
        range_query.time_range = Some(TimeRange::Day);
        assert_eq!(
            build_request(&range_query).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );

        let mut page_query = query();
        page_query.page = Some(101);
        assert_eq!(
            build_request(&page_query).unwrap_err().kind,
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
}
