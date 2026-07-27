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

pub struct MojeekEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "mojeek-web",
    display_name: "Mojeek Web",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["www.mojeek.com"],
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
    weight: 1.0,
    parser_version: "mojeek-web-html-v1",
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
    no_results: Some(".no-results, .no-results-found, #no-results"),
    item: "ul.results-standard li",
    title: "h2 a",
    url: "a.ob",
    description: Some("p.s"),
    thumbnail: None,
};

const MILLIS_PER_DAY: u64 = 86_400_000;

fn safe_search_code(value: SafeSearch) -> &'static str {
    match value {
        SafeSearch::Off => "0",
        SafeSearch::Moderate | SafeSearch::Strict => "1",
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

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

fn since_date_at(value: TimeRange, today_days: i64) -> String {
    let (year, month, day) = civil_from_days(today_days);
    let (year, month, day) = match value {
        TimeRange::Day => civil_from_days(today_days - 1),
        TimeRange::Week => civil_from_days(today_days - 7),
        TimeRange::Month => {
            let (target_year, target_month) = if month == 1 {
                (year - 1, 12)
            } else {
                (year, month - 1)
            };
            (
                target_year,
                target_month,
                day.min(days_in_month(target_year, target_month)),
            )
        }
        TimeRange::Year => {
            let target_year = year - 1;
            (
                target_year,
                month,
                day.min(days_in_month(target_year, month)),
            )
        }
    };
    format!("{year:04}{month:02}{day:02}")
}

fn build_request(
    query: &NormalizedQuery,
    request_started_at_ms: u64,
) -> Result<EngineRequest, EngineFailure> {
    let page = query.page_number();
    if page > 10 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Mojeek web supports at most ten pages",
        ));
    }

    let mut url = Url::parse("https://www.mojeek.com/search").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("q", &query.text)
            .append_pair("safe", safe_search_code(query.safe_search));
        if page > 1 {
            pairs.append_pair("s", &(10 * (page - 1)).to_string());
        }
        if let Some(time_range) = query.time_range {
            let today_days = (request_started_at_ms / MILLIS_PER_DAY) as i64;
            pairs.append_pair("since", &since_date_at(time_range, today_days));
        }
    }

    let (language, country) = locale_parts(query);
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
        cookies: BTreeMap::from([("lb".into(), language), ("arc".into(), country)]),
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
impl SearchEngine for MojeekEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let request_started_at_ms = context
            .deadline
            .expires_at_ms()
            .saturating_sub(u64::from(query.timeout_ms));
        let request = build_request(query, request_started_at_ms)?;
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
            ranking: RankingStrategy::QueryAwareV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_locale_safe_and_time_request() {
        let request = build_request(&query(), 20_300_u64 * MILLIS_PER_DAY).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(request.url.host_str(), Some("www.mojeek.com"));
        assert_eq!(parameters.get("s").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("safe").map(String::as_str), Some("1"));
        assert_eq!(
            parameters.get("since").map(String::as_str),
            Some("20250630")
        );
        assert_eq!(request.cookies.get("lb").map(String::as_str), Some("fr"));
        assert_eq!(request.cookies.get("arc").map(String::as_str), Some("CA"));
    }

    #[test]
    fn rejects_excessive_pages() {
        let mut value = query();
        value.page = Some(11);
        assert_eq!(
            build_request(&value, 20_300_u64 * MILLIS_PER_DAY)
                .unwrap_err()
                .kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn parses_normal_empty_and_changed_layout_fixtures() {
        let url = Url::parse("https://www.mojeek.com/search?q=cloudflare").unwrap();
        let normal = parse_results(
            include_bytes!("../../../fixtures/engines/mojeek-web/normal.html"),
            &url,
        )
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Mojeek result");

        let empty = parse_results(
            include_bytes!("../../../fixtures/engines/mojeek-web/empty.html"),
            &url,
        )
        .unwrap();
        assert!(empty.is_empty());

        assert!(parse_results(
            include_bytes!("../../../fixtures/engines/mojeek-web/changed-layout.html"),
            &url,
        )
        .is_err());
    }
}
