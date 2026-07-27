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
        paging: true,
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
    parser_version: "wikipedia-action-json-v1",
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
    let host = language_host(query.locale.as_deref());
    let page_size = u32::from(query.limit);
    let offset = query
        .page_number()
        .saturating_sub(1)
        .saturating_mul(page_size);
    let mut url = Url::parse(&format!("https://{host}/w/api.php")).map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("action", "query")
        .append_pair("generator", "search")
        .append_pair("gsrsearch", &query.text)
        .append_pair("gsrnamespace", "0")
        .append_pair("gsrlimit", &page_size.to_string())
        .append_pair("gsroffset", &offset.to_string())
        .append_pair("prop", "extracts|info|pageimages")
        .append_pair("exintro", "1")
        .append_pair("explaintext", "1")
        .append_pair("exlimit", "max")
        .append_pair("inprop", "url")
        .append_pair("piprop", "thumbnail")
        .append_pair("pithumbsize", "200")
        .append_pair("pilimit", "max")
        .append_pair("format", "json")
        .append_pair("formatversion", "2")
        .append_pair("utf8", "1");

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
    fn restricts_languages_to_compiled_hosts() {
        assert_eq!(language_host(Some("fr-CA")), "fr.wikipedia.org");
        assert_eq!(language_host(Some("zh-CN")), "en.wikipedia.org");
    }

    #[test]
    fn builds_bounded_action_api_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("fr.wikipedia.org"));
        assert_eq!(request.url.path(), "/w/api.php");
        assert_eq!(
            parameters.get("generator").map(String::as_str),
            Some("search")
        );
        assert_eq!(
            parameters.get("gsrsearch").map(String::as_str),
            Some("cloudflare rust")
        );
        assert_eq!(parameters.get("gsroffset").map(String::as_str), Some("10"));
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
}
