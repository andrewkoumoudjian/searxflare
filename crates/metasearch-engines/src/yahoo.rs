use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SafeSearch, SearchEngine, SourceKind, StatePolicy, TimeRange,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS, HTML_ENGINE_TIMEOUT_MS,
};
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use serde_json::Map;
use std::collections::BTreeMap;
use url::{form_urlencoded, Url};

pub struct YahooEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "yahoo-web",
    display_name: "Yahoo Web",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &[
        "search.yahoo.com",
        "ca.search.yahoo.com",
        "de.search.yahoo.com",
        "fr.search.yahoo.com",
        "uk.search.yahoo.com",
        "br.search.yahoo.com",
        "in.search.yahoo.com",
        "espanol.search.yahoo.com",
        "mx.search.yahoo.com",
        "sg.search.yahoo.com",
        "hk.search.yahoo.com",
        "tw.search.yahoo.com",
    ],
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
    weight: 0.9,
    parser_version: "yahoo-web-html-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 180,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results, #no-results, .NoResults"),
    item: "div.algo-sr",
    title: "h3",
    url: "div.compTitle a",
    description: Some("div.compText"),
    thumbnail: None,
};

fn safe_search_code(value: SafeSearch) -> &'static str {
    match value {
        SafeSearch::Off => "p",
        SafeSearch::Moderate => "i",
        SafeSearch::Strict => "r",
    }
}

fn time_range_code(value: Option<TimeRange>) -> Result<Option<&'static str>, EngineFailure> {
    match value {
        Some(TimeRange::Day) => Ok(Some("d")),
        Some(TimeRange::Week) => Ok(Some("w")),
        Some(TimeRange::Month) => Ok(Some("m")),
        Some(TimeRange::Year) => Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Yahoo web does not expose a stable one-year time filter",
        )),
        None => Ok(None),
    }
}

fn locale_parts(query: &NormalizedQuery) -> (String, String) {
    let mut parts = query.locale.as_deref().unwrap_or("en-US").split(['-', '_']);
    let language = parts.next().unwrap_or("en").to_ascii_lowercase();
    let country = query
        .country
        .as_deref()
        .or_else(|| parts.next())
        .unwrap_or("US")
        .to_ascii_uppercase();
    (language, country)
}

fn yahoo_host(language: &str, country: &str) -> &'static str {
    match country {
        "CA" => "ca.search.yahoo.com",
        "DE" => "de.search.yahoo.com",
        "FR" => "fr.search.yahoo.com",
        "GB" | "UK" => "uk.search.yahoo.com",
        "BR" => "br.search.yahoo.com",
        "IN" => "in.search.yahoo.com",
        "ES" => "espanol.search.yahoo.com",
        "MX" => "mx.search.yahoo.com",
        "SG" => "sg.search.yahoo.com",
        "HK" => "hk.search.yahoo.com",
        "TW" => "tw.search.yahoo.com",
        _ if language == "zh" => "hk.search.yahoo.com",
        _ => "search.yahoo.com",
    }
}

fn language_code(language: &str, country: &str) -> &'static str {
    match (language, country) {
        ("zh", "TW" | "HK") => "zh_cht",
        ("zh", _) => "zh_chs",
        ("ar", _) => "ar",
        ("de", _) => "de",
        ("es", _) => "es",
        ("fr", _) => "fr",
        ("it", _) => "it",
        ("ja", _) => "ja",
        ("ko", _) => "ko",
        ("nl", _) => "nl",
        ("pl", _) => "pl",
        ("pt", _) => "pt",
        ("ru", _) => "ru",
        ("sv", _) => "sv",
        ("tr", _) => "tr",
        _ => "en",
    }
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Yahoo web supports at most ten pages",
        ));
    }

    let (language, country) = locale_parts(query);
    let host = yahoo_host(&language, &country);
    let language = language_code(&language, &country);
    let mut url = Url::parse(&format!("https://{host}/search")).map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("p", &query.text);
        if let Some(code) = time_range_code(query.time_range)? {
            pairs.append_pair("btf", code);
        }
        if page == 1 {
            pairs.append_pair("iscqry", "");
        } else {
            pairs
                .append_pair("b", &(page * 7 + 1).to_string())
                .append_pair("pz", "7")
                .append_pair("bct", "0")
                .append_pair("xargs", "0");
        }
    }

    let search_cookie = format!(
        "v=1&vm={}&fl=1&vl=lang_{}&pn=10&rw=new&userset=1",
        safe_search_code(query.safe_search),
        language
    );
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
        cookies: BTreeMap::from([("sB".into(), search_cookie)]),
        body: None,
        accepted_content_types: &["text/html", "application/xhtml+xml"],
    })
}

fn decode_component(value: &str) -> String {
    form_urlencoded::parse(format!("value={value}").as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| value.to_owned())
}

fn unwrap_yahoo_url(value: &str) -> String {
    let Some(marker) = value.find("/RU=") else {
        return value.to_owned();
    };
    let start = marker + 4;
    let end = ["/RK=", "/RS="]
        .iter()
        .filter_map(|ending| value[start..].find(ending).map(|offset| start + offset))
        .min();
    let Some(end) = end else {
        return value.to_owned();
    };
    let decoded = decode_component(&value[start..end]);
    if Url::parse(&decoded).is_ok() {
        decoded
    } else {
        value.to_owned()
    }
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
            url: unwrap_yahoo_url(&result.url),
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
impl SearchEngine for YahooEngine {
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
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_locale_safe_and_time_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("ca.search.yahoo.com"));
        assert_eq!(parameters.get("b").map(String::as_str), Some("15"));
        assert_eq!(parameters.get("pz").map(String::as_str), Some("7"));
        assert_eq!(parameters.get("btf").map(String::as_str), Some("m"));
        assert!(request.cookies["sB"].contains("vm=r"));
        assert!(request.cookies["sB"].contains("vl=lang_fr"));
    }

    #[test]
    fn rejects_unsupported_year_filter_and_excessive_pages() {
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
    fn unwraps_tracking_urls() {
        assert_eq!(
            unwrap_yahoo_url("https://r.search.yahoo.com/_ylt=x/RU=https%3A%2F%2Fexample.com%2Fitem%3Fa%3D1/RK=2/RS=x"),
            "https://example.com/item?a=1"
        );
    }

    #[test]
    fn parses_normal_empty_and_changed_layout_fixtures() {
        let url = Url::parse("https://search.yahoo.com/search?p=cloudflare").unwrap();
        let normal = parse_results(
            include_bytes!("../../../fixtures/engines/yahoo-web/normal.html"),
            &url,
        )
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Yahoo result");
        assert_eq!(normal[0].url, "https://example.com/yahoo");

        let empty = parse_results(
            include_bytes!("../../../fixtures/engines/yahoo-web/empty.html"),
            &url,
        )
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(
            include_bytes!("../../../fixtures/engines/yahoo-web/changed-layout.html"),
            &url,
        )
        .is_err());
    }
}
