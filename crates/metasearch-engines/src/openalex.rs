use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct OpenAlexEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "openalex",
    display_name: "OpenAlex",
    categories: &["science"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["api.openalex.org"],
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
    weight: 1.0,
    parser_version: "openalex-works-json-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 900,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.time_range.is_some() || query.page_number() > 100 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "OpenAlex supports at most 100 pages and no time-range mapping in this adapter",
        ));
    }
    let mut url = Url::parse("https://api.openalex.org/works").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("search", &query.text)
        .append_pair("page", &query.page_number().to_string())
        .append_pair("per-page", &query.limit.to_string())
        .append_pair("sort", "relevance_score:desc")
        .append_pair(
            "select",
            "id,doi,title,publication_date,primary_location,authorships,cited_by_count,open_access",
        );
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([("accept".into(), "application/json".into())]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json"],
    })
}

fn parse_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    let items = root
        .get("results")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "OpenAlex response is missing results",
            )
        })?;
    let mut results = Vec::new();
    for item in items {
        let Some(title) = item
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let url = item
            .get("doi")
            .and_then(Value::as_str)
            .or_else(|| item.get("id").and_then(Value::as_str));
        let Some(url) = url.filter(|value| Url::parse(value).is_ok()) else {
            continue;
        };
        let authors = item
            .get("authorships")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|authorship| {
                authorship
                    .pointer("/author/display_name")
                    .and_then(Value::as_str)
            })
            .take(20)
            .collect::<Vec<_>>();
        let venue = item
            .pointer("/primary_location/source/display_name")
            .and_then(Value::as_str);
        results.push(ProviderResult {
            url: url.into(),
            title: title.into(),
            content: [venue.unwrap_or_default(), &authors.join(", ")]
                .into_iter()
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" — "),
            published_at: item
                .get("publication_date")
                .and_then(Value::as_str)
                .map(str::to_owned),
            thumbnail: None,
            category: "science".into(),
            metadata: Map::from_iter([
                (
                    "cited_by_count".into(),
                    json!(item.get("cited_by_count").and_then(Value::as_u64)),
                ),
                (
                    "is_open_access".into(),
                    json!(item
                        .pointer("/open_access/is_oa")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)),
                ),
            ]),
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for OpenAlexEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let response = context
            .http
            .send(&DESCRIPTOR, build_request(query)?, context.deadline)
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

    #[test]
    fn parses_works() {
        let results = parse_results(
            br#"{"results":[{"id":"https://openalex.org/W1","doi":"https://doi.org/10.1/x","title":"A work","publication_date":"2026-01-01","authorships":[],"cited_by_count":2,"open_access":{"is_oa":true}}]}"#,
        )
        .unwrap();
        assert_eq!(results[0].title, "A work");
    }

    #[test]
    fn builds_public_works_request_without_api_key() {
        use metasearch_core::{RankingStrategy, SafeSearch};

        let request = build_request(&NormalizedQuery {
            text: "machine learning".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 10,
            locale: None,
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
        })
        .unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert!(!parameters.contains_key("api_key"));
        assert_eq!(
            parameters.get("sort").map(String::as_str),
            Some("relevance_score:desc")
        );
    }
}
