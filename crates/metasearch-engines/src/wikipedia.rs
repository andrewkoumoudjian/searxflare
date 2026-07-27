use crate::text::strip_markup;
use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct WikipediaEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "wikipedia",
    display_name: "Wikipedia",
    categories: &["reference"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &[
        "en.wikipedia.org",
        "fr.wikipedia.org",
        "de.wikipedia.org",
        "es.wikipedia.org",
    ],
    capabilities: EngineCapabilities {
        paging: false,
        locale: true,
        country: false,
        safe_search: false,
        time_range: false,
    },
    timeout_ms: DEFAULT_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.15,
    parser_version: "wikipedia-rest-summary-json-v2",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 1_800,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn language_host(locale: Option<&str>) -> &'static str {
    let language = locale
        .and_then(|locale| locale.split(['-', '_']).next())
        .unwrap_or("en");
    match language {
        "fr" => "fr.wikipedia.org",
        "de" => "de.wikipedia.org",
        "es" => "es.wikipedia.org",
        _ => "en.wikipedia.org",
    }
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.page_number() > 1 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Wikipedia REST summaries do not support paging",
        ));
    }
    let host = language_host(query.locale.as_deref());
    let mut url =
        Url::parse(&format!("https://{host}/api/rest_v1/page/summary")).map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
    url.path_segments_mut()
        .map_err(|_| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::Internal,
                "Wikipedia summary URL cannot accept path segments",
            )
        })?
        .push(&query.text);

    let agent = "searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)";
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            ("accept".into(), "application/json".into()),
            (
                "accept-language".into(),
                query
                    .locale
                    .clone()
                    .unwrap_or_else(|| "en-US,en;q=0.8".into()),
            ),
            ("api-user-agent".into(), agent.into()),
            ("user-agent".into(), agent.into()),
        ]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json"],
    })
}

fn classify_api_error(root: &Value) -> Option<EngineFailure> {
    let error = root.get("error")?;
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let message = error
        .get("info")
        .and_then(Value::as_str)
        .unwrap_or("Wikipedia returned an unsuccessful API response");
    let kind = match code.to_ascii_lowercase().as_str() {
        "maxlag" | "ratelimited" => FailureKind::EngineRateLimited,
        _ => FailureKind::EngineParseFailed,
    };
    Some(EngineFailure::new(
        DESCRIPTOR.id,
        kind,
        format!("{code}: {message}"),
    ))
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
    if root.get("content_urls").is_some() {
        let title = root
            .pointer("/titles/display")
            .or_else(|| root.get("displaytitle"))
            .or_else(|| root.get("title"))
            .and_then(Value::as_str)
            .map(strip_markup)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineParseFailed,
                    "Wikipedia summary is missing title",
                )
            })?;
        let url = root
            .pointer("/content_urls/desktop/page")
            .and_then(Value::as_str)
            .filter(|value| Url::parse(value).is_ok())
            .ok_or_else(|| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineParseFailed,
                    "Wikipedia summary is missing its canonical page URL",
                )
            })?;
        let content = root
            .get("extract")
            .or_else(|| root.get("description"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        return Ok(vec![ProviderResult {
            url: url.into(),
            title,
            content,
            published_at: root
                .get("timestamp")
                .and_then(Value::as_str)
                .map(str::to_owned),
            thumbnail: root
                .pointer("/thumbnail/source")
                .and_then(Value::as_str)
                .map(str::to_owned),
            category: "reference".into(),
            metadata: Map::new(),
            engine_id: DESCRIPTOR.id.into(),
            position: 1,
            engine_weight: DESCRIPTOR.weight,
        }]);
    }

    let Some(query) = root.get("query") else {
        return Ok(Vec::new());
    };
    let pages = query
        .get("pages")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Wikipedia response is missing query.pages",
            )
        })?;

    let mut results = Vec::new();
    for page in pages {
        let Some(title) = page
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|title| !title.is_empty())
        else {
            continue;
        };
        let Some(url) = page
            .get("fullurl")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| Url::parse(url).is_ok())
        else {
            continue;
        };
        let content = page
            .get("extract")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let thumbnail = page
            .pointer("/thumbnail/source")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut metadata = Map::new();
        if let Some(page_id) = page.get("pageid").and_then(Value::as_i64) {
            metadata.insert("page_id".into(), json!(page_id));
        }

        results.push(ProviderResult {
            url: url.to_owned(),
            title: title.to_owned(),
            content,
            published_at: None,
            thumbnail,
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
impl SearchEngine for WikipediaEngine {
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
    use metasearch_core::{RankingStrategy, SafeSearch};

    fn query() -> NormalizedQuery {
        NormalizedQuery {
            text: "cloudflare rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(1),
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
    fn restricts_languages_to_compiled_hosts() {
        assert_eq!(language_host(Some("fr-CA")), "fr.wikipedia.org");
        assert_eq!(language_host(Some("zh-CN")), "en.wikipedia.org");
    }

    #[test]
    fn builds_rest_summary_request() {
        let request = build_request(&query()).unwrap();
        assert_eq!(request.url.host_str(), Some("fr.wikipedia.org"));
        assert_eq!(
            request.url.path(),
            "/api/rest_v1/page/summary/cloudflare%20rust"
        );
    }

    #[test]
    fn parses_normal_empty_and_error_responses() {
        let normal = br#"{
          "batchcomplete": true,
          "query": {
            "pages": [{
              "pageid": 42,
              "ns": 0,
              "title": "Cloudflare",
              "extract": "Cloudflare is a web infrastructure company.",
              "fullurl": "https://en.wikipedia.org/wiki/Cloudflare",
              "thumbnail": {"source": "https://upload.wikimedia.org/example.png"}
            }]
          }
        }"#;
        let results = parse_results(normal).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Cloudflare");
        assert_eq!(results[0].metadata.get("page_id"), Some(&json!(42)));

        assert!(parse_results(br#"{"batchcomplete":true}"#)
            .unwrap()
            .is_empty());
        assert_eq!(
            parse_results(br#"{"error":{"code":"maxlag","info":"Waiting for hosts"}}"#)
                .unwrap_err()
                .kind,
            FailureKind::EngineRateLimited
        );
        assert!(parse_results(br#"{"query":{"pages":{}}}"#).is_err());
    }

    #[test]
    fn parses_rest_summary_response() {
        let results = parse_results(
            br#"{"type":"standard","title":"Cloudflare","displaytitle":"<b>Cloudflare</b>","extract":"Web infrastructure company.","timestamp":"2026-07-27T00:00:00Z","content_urls":{"desktop":{"page":"https://en.wikipedia.org/wiki/Cloudflare"}},"thumbnail":{"source":"https://upload.wikimedia.org/example.png"}}"#,
        )
        .unwrap();
        assert_eq!(results[0].title, "Cloudflare");
        assert_eq!(results[0].position, 1);
    }
}
