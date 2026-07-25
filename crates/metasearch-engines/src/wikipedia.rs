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

pub struct WikipediaEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "wikipedia",
    display_name: "Wikipedia",
    categories: &["reference"],
    source_kind: SourceKind::Html,
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
    timeout_ms: HTML_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.15,
    parser_version: "wikipedia-html-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 1_800,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".mw-search-nonefound"),
    item: ".mw-search-result",
    title: ".mw-search-result-heading a",
    url: ".mw-search-result-heading a",
    description: Some(".searchresult"),
    thumbnail: Some(".searchResultImage img"),
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
        let host = language_host(query.locale.as_deref());
        let mut url = Url::parse(&format!("https://{host}/w/index.php")).map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
        let offset = query.page_number().saturating_sub(1).saturating_mul(10);
        url.query_pairs_mut()
            .append_pair("search", &query.text)
            .append_pair("title", "Special:Search")
            .append_pair("fulltext", "1")
            .append_pair("ns0", "1")
            .append_pair("limit", "10")
            .append_pair("offset", &offset.to_string());
        let request = EngineRequest {
            method: EngineMethod::Get,
            url: url.clone(),
            headers: BTreeMap::from([
                (
                    "accept".into(),
                    "text/html,application/xhtml+xml;q=0.9".into(),
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
                    "searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
                ),
            ]),
            cookies: BTreeMap::new(),
            body: None,
            accepted_content_types: &["text/html", "application/xhtml+xml"],
        };
        let response = context
            .http
            .send(&DESCRIPTOR, request, context.deadline)
            .await?;
        let parsed =
            parse_selector_results(&response.body, &url, &SELECTORS, 10).map_err(|error| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineParseFailed,
                    error.to_string(),
                )
            })?;
        let results = parsed
            .into_iter()
            .enumerate()
            .map(|(index, result)| ProviderResult {
                url: result.url,
                title: result.title,
                content: result.description.unwrap_or_default(),
                published_at: None,
                thumbnail: result.thumbnail,
                category: "reference".into(),
                metadata: Map::new(),
                engine_id: DESCRIPTOR.id.into(),
                position: (index + 1) as u32,
                engine_weight: DESCRIPTOR.weight,
            })
            .collect();
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

    #[test]
    fn restricts_languages_to_compiled_hosts() {
        assert_eq!(language_host(Some("fr-CA")), "fr.wikipedia.org");
        assert_eq!(language_host(Some("zh-CN")), "en.wikipedia.org");
    }
}
