use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_MAX_BODY_BYTES,
    DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS, HTML_ENGINE_TIMEOUT_MS,
};
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use serde_json::Map;
use std::collections::BTreeMap;
use url::Url;

pub struct BaiduEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "baidu-web",
    display_name: "Baidu Web",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &["www.baidu.com"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: false,
        country: false,
        safe_search: false,
        time_range: false,
    },
    timeout_ms: HTML_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 0.9,
    parser_version: "baidu-web-html-v1",
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
    no_results: Some("div.nors, div#content_none, .search-noresult"),
    item: "div.result, div.c-container",
    title: "h3 a",
    url: "h3 a",
    description: Some("div.c-abstract, div.content-right_8Zs40, span.content-right_8Zs40"),
    thumbnail: None,
};

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Baidu web supports at most ten pages",
        ));
    }
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Baidu time filters are not exposed by this adapter",
        ));
    }

    let result_count = u32::from(query.limit.min(10));
    let offset = page.saturating_sub(1).saturating_mul(10);
    let mut url = Url::parse("https://www.baidu.com/s").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("wd", &query.text)
        .append_pair("pn", &offset.to_string())
        .append_pair("rn", &result_count.to_string())
        .append_pair("ie", "utf-8");

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            (
                "accept".into(),
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".into(),
            ),
            ("accept-language".into(), "zh-CN,zh;q=0.9,en;q=0.6".into()),
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
        .enumerate()
        .map(|(index, result)| ProviderResult {
            url: result.url,
            title: result.title,
            content: result.description.unwrap_or_default(),
            published_at: None,
            thumbnail: None,
            category: "general".into(),
            metadata: Map::new(),
            engine_id: DESCRIPTOR.id.into(),
            position: (index + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        })
        .collect())
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for BaiduEngine {
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
            text: "cloudflare rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 20,
            locale: Some("zh-CN".into()),
            country: Some("CN".into()),
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_paging_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("www.baidu.com"));
        assert_eq!(
            parameters.get("wd").map(String::as_str),
            Some("cloudflare rust")
        );
        assert_eq!(parameters.get("pn").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("rn").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("ie").map(String::as_str), Some("utf-8"));
    }

    #[test]
    fn rejects_time_range_and_excessive_pages() {
        let mut value = query();
        value.time_range = Some(TimeRange::Day);
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
    fn parses_normal_empty_and_changed_layout_fixtures() {
        let url = Url::parse("https://www.baidu.com/s?wd=cloudflare").unwrap();
        let normal = parse_results(
            include_bytes!("../../../fixtures/engines/baidu-web/normal.html"),
            &url,
        )
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Baidu result");

        let empty = parse_results(
            include_bytes!("../../../fixtures/engines/baidu-web/empty.html"),
            &url,
        )
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(
            include_bytes!("../../../fixtures/engines/baidu-web/changed-layout.html"),
            &url,
        )
        .is_err());
    }
}
