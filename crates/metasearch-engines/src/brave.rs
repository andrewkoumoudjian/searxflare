use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SafeSearch, SearchEngine, SourceKind, StatePolicy, TimeRange,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS, HTML_ENGINE_TIMEOUT_MS,
};
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use serde_json::Map;
use std::collections::BTreeMap;
use url::Url;

pub struct BraveEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "brave-web",
    display_name: "Brave Web",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["search.brave.com"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: true,
        country: true,
        safe_search: true,
        time_range: true,
    },
    timeout_ms: HTML_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.1,
    parser_version: "brave-web-html-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 180,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results, #no-results, .search-no-results"),
    item: "div.snippet",
    title: "div.title",
    url: "a",
    description: Some("div.content"),
    thumbnail: Some("a.thumbnail img"),
};

fn safe_search_cookie(value: SafeSearch) -> &'static str {
    match value {
        SafeSearch::Off => "off",
        SafeSearch::Moderate => "moderate",
        SafeSearch::Strict => "strict",
    }
}

fn time_range_code(value: Option<TimeRange>) -> Option<&'static str> {
    match value {
        Some(TimeRange::Day) => Some("pd"),
        Some(TimeRange::Week) => Some("pw"),
        Some(TimeRange::Month) => Some("pm"),
        Some(TimeRange::Year) => Some("py"),
        None => None,
    }
}

fn locale_parts(query: &NormalizedQuery) -> (String, String) {
    let mut parts = query
        .locale
        .as_deref()
        .unwrap_or("en-US")
        .split(['-', '_']);
    let language = parts.next().unwrap_or("en").to_ascii_lowercase();
    let country = query
        .country
        .as_deref()
        .or_else(|| parts.next())
        .unwrap_or("US")
        .to_ascii_lowercase();
    (format!("{language}-{country}"), country)
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Brave web supports at most ten pages",
        ));
    }

    let mut url = Url::parse("https://search.brave.com/search").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("q", &query.text).append_pair("source", "web");
        if page > 1 {
            pairs.append_pair("offset", &(page - 1).to_string());
        }
        if let Some(code) = time_range_code(query.time_range) {
            pairs.append_pair("tf", code);
        }
    }

    let (ui_lang, country) = locale_parts(query);
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            (
                "accept".into(),
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".into(),
            ),
            (
                "accept-language".into(),
                query
                    .locale
                    .clone()
                    .unwrap_or_else(|| "en-US,en;q=0.8".into()),
            ),
            (
                "user-agent".into(),
                "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::from([
            ("safesearch".into(), safe_search_cookie(query.safe_search).into()),
            ("useLocation".into(), "0".into()),
            ("summarizer".into(), "0".into()),
            ("country".into(), country),
            ("ui_lang".into(), ui_lang),
        ]),
        body: None,
        accepted_content_types: &["text/html", "application/xhtml+xml"],
    })
}

fn parse_results(body: &[u8], request_url: &Url) -> Result<Vec<ProviderResult>, EngineFailure> {
    let parsed = parse_selector_results(body, request_url, &SELECTORS, 20).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;

    Ok(parsed
        .into_iter()
        .filter(|result| {
            Url::parse(&result.url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .as_deref()
                != Some("search.brave.com")
        })
        .enumerate()
        .map(|(index, result)| ProviderResult {
            url: result.url,
            title: result.title,
            content: result.description.unwrap_or_default(),
            published_at: None,
            thumbnail: result.thumbnail,
            category: "general".into(),
            metadata: Map::new(),
            engine_id: DESCRIPTOR.id.into(),
            position: (index + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        })
        .collect())
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for BraveEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let request = build_request(query)?;
        let request_url = request.url.clone();
        let response = context
            .http
            .send(&DESCRIPTOR, request, context.deadline)
            .await?;
        let results = parse_results(&response.body, &request_url)?;

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
    use metasearch_core::RankingStrategy;

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
            safe_search: SafeSearch::Strict,
            time_range: Some(TimeRange::Month),
            ranking: RankingStrategy::RrfV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_reference_request() {
        let request = build_request(&query()).unwrap();
        assert_eq!(request.url.host_str(), Some("search.brave.com"));
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(parameters.get("offset").map(String::as_str), Some("1"));
        assert_eq!(parameters.get("tf").map(String::as_str), Some("pm"));
        assert_eq!(request.cookies.get("country").map(String::as_str), Some("ca"));
        assert_eq!(request.cookies.get("ui_lang").map(String::as_str), Some("fr-ca"));
        assert_eq!(
            request.cookies.get("safesearch").map(String::as_str),
            Some("strict")
        );
    }

    #[test]
    fn parses_normal_empty_and_changed_fixtures() {
        let url = Url::parse("https://search.brave.com/search?q=cloudflare").unwrap();
        let normal = parse_results(
            include_bytes!("../../../fixtures/engines/brave-web/normal.html"),
            &url,
        )
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Brave result");

        let empty = parse_results(
            include_bytes!("../../../fixtures/engines/brave-web/empty.html"),
            &url,
        )
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(
            include_bytes!("../../../fixtures/engines/brave-web/changed-layout.html"),
            &url,
        )
        .is_err());
    }
}