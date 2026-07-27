use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct GitHubEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "github",
    display_name: "GitHub",
    categories: &["code"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["api.github.com"],
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
    parser_version: "github-rest-json-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 300,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "GitHub time-range filtering is not exposed by this adapter",
        ));
    }

    let mut url = Url::parse("https://api.github.com/search/repositories").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("q", &query.text)
        .append_pair("sort", "stars")
        .append_pair("order", "desc")
        .append_pair("page", &query.page_number().to_string())
        .append_pair("per_page", &query.limit.to_string());

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            ("accept".into(), "application/vnd.github+json".into()),
            ("x-github-api-version".into(), "2026-03-10".into()),
            (
                "user-agent".into(),
                "searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json", "application/vnd.github+json"],
    })
}

fn optional_string(item: &Value, field: &str) -> Option<String> {
    item.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn classify_api_error(root: &Value) -> Option<EngineFailure> {
    let message = root.get("message").and_then(Value::as_str)?;
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("rate limit") || lower.contains("secondary rate") {
        FailureKind::EngineRateLimited
    } else if lower.contains("abuse") || lower.contains("temporarily blocked") {
        FailureKind::EngineChallenged
    } else {
        FailureKind::EngineParseFailed
    };
    Some(EngineFailure::new(DESCRIPTOR.id, kind, message))
}

fn parse_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if let Some(failure) = classify_api_error(&root) {
        return Err(failure);
    }

    let items = root.get("items").and_then(Value::as_array).ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            "GitHub response is missing items",
        )
    })?;

    let mut results = Vec::new();
    for item in items {
        let Some(title) = optional_string(item, "full_name") else {
            continue;
        };
        let Some(url) =
            optional_string(item, "html_url").filter(|candidate| Url::parse(candidate).is_ok())
        else {
            continue;
        };

        let language = optional_string(item, "language");
        let description = optional_string(item, "description");
        let content = [language.clone(), description]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" / ");
        let owner = item.get("owner");
        let thumbnail = owner
            .and_then(|value| value.get("avatar_url"))
            .and_then(Value::as_str)
            .map(str::to_owned);

        let mut metadata = Map::new();
        for (field, metadata_key) in [
            ("id", "repository_id"),
            ("stargazers_count", "stars"),
            ("forks_count", "forks"),
            ("open_issues_count", "open_issues"),
        ] {
            if let Some(value) = item.get(field).and_then(Value::as_u64) {
                metadata.insert(metadata_key.into(), json!(value));
            }
        }
        if let Some(value) = language {
            metadata.insert("language".into(), Value::String(value));
        }
        if let Some(value) = owner
            .and_then(|owner| owner.get("login"))
            .and_then(Value::as_str)
        {
            metadata.insert("owner".into(), Value::String(value.into()));
        }
        if let Some(value) = item.get("topics").and_then(Value::as_array) {
            metadata.insert("topics".into(), Value::Array(value.clone()));
        }
        if let Some(value) = item
            .pointer("/license/spdx_id")
            .and_then(Value::as_str)
            .filter(|value| *value != "NOASSERTION")
        {
            metadata.insert("license_spdx".into(), Value::String(value.into()));
        }
        for field in ["homepage", "clone_url", "default_branch"] {
            if let Some(value) = optional_string(item, field) {
                metadata.insert(field.into(), Value::String(value));
            }
        }

        results.push(ProviderResult {
            url,
            title,
            content,
            published_at: optional_string(item, "updated_at")
                .or_else(|| optional_string(item, "created_at")),
            thumbnail,
            category: "code".into(),
            metadata,
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for GitHubEngine {
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
            text: "cloudflare workers rust".into(),
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
        }
    }

    #[test]
    fn builds_bounded_repository_search_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("api.github.com"));
        assert_eq!(request.url.path(), "/search/repositories");
        assert_eq!(
            parameters.get("q").map(String::as_str),
            Some("cloudflare workers rust")
        );
        assert_eq!(parameters.get("page").map(String::as_str), Some("2"));
        assert_eq!(parameters.get("per_page").map(String::as_str), Some("10"));
        assert_eq!(
            request
                .headers
                .get("x-github-api-version")
                .map(String::as_str),
            Some("2026-03-10")
        );
    }

    #[test]
    fn rejects_unsupported_time_range() {
        let mut value = query();
        value.time_range = Some(TimeRange::Year);
        assert_eq!(
            build_request(&value).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn parses_normal_empty_schema_and_rate_limit_fixtures() {
        let normal = parse_results(include_bytes!(
            "../../../fixtures/engines/github/normal.json"
        ))
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "cloudflare/workers-rs");
        assert_eq!(normal[0].metadata["stars"], 5000);
        assert_eq!(normal[0].metadata["license_spdx"], "Apache-2.0");

        let empty = parse_results(include_bytes!(
            "../../../fixtures/engines/github/empty.json"
        ))
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(include_bytes!(
            "../../../fixtures/engines/github/changed-schema.json"
        ))
        .is_err());
        assert_eq!(
            parse_results(include_bytes!(
                "../../../fixtures/engines/github/rate-limited.json"
            ))
            .unwrap_err()
            .kind,
            FailureKind::EngineRateLimited
        );
    }
}
