use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SafeSearch, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct QwantEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "qwant-web",
    display_name: "Qwant Web",
    categories: &["general"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["api.qwant.com", "www.qwant.com"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: true,
        country: true,
        safe_search: true,
        time_range: false,
    },
    timeout_ms: DEFAULT_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.05,
    parser_version: "qwant-web-json-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 180,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn safe_search_code(value: SafeSearch) -> &'static str {
    match value {
        SafeSearch::Off => "0",
        SafeSearch::Moderate => "1",
        SafeSearch::Strict => "2",
    }
}

fn locale_code(query: &NormalizedQuery) -> String {
    let mut parts = query.locale.as_deref().unwrap_or("en-US").split(['-', '_']);
    let language = parts.next().unwrap_or("en").to_ascii_lowercase();
    let country = query
        .country
        .as_deref()
        .or_else(|| parts.next())
        .unwrap_or("US")
        .to_ascii_uppercase();
    format!("{language}_{country}")
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 5 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Qwant web supports at most five pages",
        ));
    }
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Qwant web does not expose a stable time-range parameter",
        ));
    }

    let count = 10_u32;
    let offset = page.saturating_sub(1).saturating_mul(count);
    let mut url = Url::parse("https://api.qwant.com/v3/search/web").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("q", &query.text)
        .append_pair("count", &count.to_string())
        .append_pair("locale", &locale_code(query))
        .append_pair("offset", &offset.to_string())
        .append_pair("tgp", "1")
        .append_pair("device", "desktop")
        .append_pair("safesearch", safe_search_code(query.safe_search))
        .append_pair("displayed", "true")
        .append_pair("llm", "true");

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            ("origin".into(), "https://www.qwant.com".into()),
            ("referer".into(), "https://www.qwant.com/".into()),
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

fn classify_api_error(root: &Value) -> EngineFailure {
    let data = root.get("data").unwrap_or(&Value::Null);
    let error_code = data.get("error_code").and_then(Value::as_i64);
    if error_code == Some(24) {
        return EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineRateLimited,
            "Qwant reported rate limiting",
        );
    }
    if root.get("url").is_some_and(|value| !value.is_null()) {
        return EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineChallenged,
            "Qwant redirected the request to an anti-bot challenge",
        )
        .with_retryable(false);
    }

    let message = data
        .get("message")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "Qwant returned an unsuccessful API response".into());
    EngineFailure::new(DESCRIPTOR.id, FailureKind::EngineParseFailed, message)
}

fn collect_web_items(root: &Value) -> Result<Vec<&Value>, EngineFailure> {
    let items = root.pointer("/data/result/items").ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            "Qwant response is missing data.result.items",
        )
    })?;

    if let Some(direct) = items.as_array() {
        return Ok(direct.iter().collect());
    }

    let mainline = items
        .get("mainline")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Qwant response is missing the mainline result array",
            )
        })?;

    let mut output = Vec::new();
    for row in mainline {
        if row.get("type").and_then(Value::as_str).unwrap_or("web") != "web" {
            continue;
        }
        if let Some(row_items) = row.get("items").and_then(Value::as_array) {
            output.extend(row_items.iter());
        }
    }
    Ok(output)
}

fn parse_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if root.get("status").and_then(Value::as_str) != Some("success") {
        return Err(classify_api_error(&root));
    }

    let items = collect_web_items(&root)?;
    let mut results = Vec::new();
    for item in items {
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let result_url = item
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let valid_url = Url::parse(result_url)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https"));
        if title.is_empty() || valid_url.is_none() {
            continue;
        }

        let mut metadata = Map::new();
        if let Some(source) = item.get("source").and_then(Value::as_str) {
            metadata.insert("source".into(), Value::String(source.into()));
        }
        results.push(ProviderResult {
            url: result_url.into(),
            title: title.into(),
            content: item
                .get("desc")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            published_at: None,
            thumbnail: None,
            category: "general".into(),
            metadata,
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
        if results.len() >= 20 {
            break;
        }
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for QwantEngine {
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
    use metasearch_core::{RankingStrategy, TimeRange};

    fn query() -> NormalizedQuery {
        NormalizedQuery {
            text: "cloudflare rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 10,
            locale: Some("fr-CA".into()),
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::RrfV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_reference_request_parameters() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(parameters.get("locale").map(String::as_str), Some("fr_CA"));
        assert_eq!(parameters.get("offset").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("safesearch").map(String::as_str), Some("1"));
        assert_eq!(
            request.headers.get("origin").map(String::as_str),
            Some("https://www.qwant.com")
        );
    }

    #[test]
    fn rejects_time_range_and_excessive_pages() {
        let mut time_query = query();
        time_query.time_range = Some(TimeRange::Day);
        assert_eq!(
            build_request(&time_query).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );

        let mut page_query = query();
        page_query.page = Some(6);
        assert_eq!(
            build_request(&page_query).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn parses_normal_empty_and_failure_fixtures() {
        let normal = parse_results(include_bytes!(
            "../../../fixtures/engines/qwant-web/normal.json"
        ))
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Qwant result");

        let empty = parse_results(include_bytes!(
            "../../../fixtures/engines/qwant-web/empty.json"
        ))
        .unwrap();
        assert!(empty.is_empty());

        assert_eq!(
            parse_results(include_bytes!(
                "../../../fixtures/engines/qwant-web/rate-limited.json"
            ))
            .unwrap_err()
            .kind,
            FailureKind::EngineRateLimited
        );
        assert!(parse_results(include_bytes!(
            "../../../fixtures/engines/qwant-web/changed-layout.json"
        ))
        .is_err());
    }
}
