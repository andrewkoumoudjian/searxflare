use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct GrokipediaEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "grokipedia",
    display_name: "Grokipedia",
    categories: &["general", "reference"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &["grokipedia.com"],
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
    weight: 0.9,
    parser_version: "grokipedia-full-text-json-v2",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 300,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Grokipedia supports at most ten pages",
        ));
    }
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Grokipedia time filters are not exposed by this adapter",
        ));
    }

    let count = u32::from(query.limit.min(20));
    let offset = page.saturating_sub(1).saturating_mul(count);
    let mut url = Url::parse("https://grokipedia.com/api/full-text-search").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("query", &query.text)
        .append_pair("limit", &count.to_string())
        .append_pair("offset", &offset.to_string());

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            (
                "user-agent".into(),
                "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json", "text/json"],
    })
}

fn result_items(root: &Value) -> Result<&Vec<Value>, EngineFailure> {
    if let Some(items) = root.as_array() {
        return Ok(items);
    }
    root.get("results")
        .and_then(Value::as_array)
        .or_else(|| root.pointer("/data/results").and_then(Value::as_array))
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Grokipedia response is missing a results array",
            )
        })
}

fn optional_string(item: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        item.get(*field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn page_url(item: &Value) -> Option<String> {
    if let Some(url) = optional_string(item, &["url", "page_url"]) {
        if Url::parse(&url).is_ok() {
            return Some(url);
        }
    }
    let slug = optional_string(item, &["slug"])?;
    let mut url = Url::parse("https://grokipedia.com/page").ok()?;
    url.path_segments_mut().ok()?.push(&slug);
    Some(url.into())
}

fn parse_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if let Some(message) = root.get("message").and_then(Value::as_str) {
        let kind = if message.to_ascii_lowercase().contains("rate limit") {
            FailureKind::EngineRateLimited
        } else {
            FailureKind::EngineParseFailed
        };
        return Err(EngineFailure::new(DESCRIPTOR.id, kind, message));
    }

    let mut results = Vec::new();
    for item in result_items(&root)? {
        let Some(title) = optional_string(item, &["title", "name"]) else {
            continue;
        };
        let Some(url) = page_url(item) else {
            continue;
        };
        let mut metadata = Map::new();
        for field in ["id", "slug"] {
            if let Some(value) = item.get(field) {
                metadata.insert(field.into(), value.clone());
            }
        }
        results.push(ProviderResult {
            url,
            title,
            content: optional_string(item, &["snippet", "description", "content"])
                .unwrap_or_default(),
            published_at: optional_string(item, &["updated_at", "published_at"]),
            thumbnail: optional_string(item, &["thumbnail", "image_url"]),
            category: "reference".into(),
            metadata,
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for GrokipediaEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let request = build_request(query)?;
        let response = context
            .http
            .send(&DESCRIPTOR, request, context.deadline)
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
            text: "cloudflare".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 7,
            locale: None,
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_search_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("grokipedia.com"));
        assert_eq!(request.url.path(), "/api/full-text-search");
        assert_eq!(
            parameters.get("query").map(String::as_str),
            Some("cloudflare")
        );
        assert_eq!(parameters.get("limit").map(String::as_str), Some("7"));
        assert_eq!(parameters.get("offset").map(String::as_str), Some("7"));
    }

    #[test]
    fn rejects_time_range_and_excessive_pages() {
        let mut value = query();
        value.time_range = Some(TimeRange::Year);
        assert_eq!(
            build_request(&value).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
        value.time_range = None;
        value.page = Some(11);
        assert_eq!(
            build_request(&value).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn parses_normal_empty_schema_and_rate_limit_fixtures() {
        let normal = parse_results(include_bytes!(
            "../../../fixtures/engines/grokipedia/normal.json"
        ))
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Cloudflare");
        assert_eq!(normal[0].url, "https://grokipedia.com/page/cloudflare");

        let empty = parse_results(include_bytes!(
            "../../../fixtures/engines/grokipedia/empty.json"
        ))
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(include_bytes!(
            "../../../fixtures/engines/grokipedia/changed-schema.json"
        ))
        .is_err());
        assert_eq!(
            parse_results(include_bytes!(
                "../../../fixtures/engines/grokipedia/rate-limited.json"
            ))
            .unwrap_err()
            .kind,
            FailureKind::EngineRateLimited
        );
    }
}
