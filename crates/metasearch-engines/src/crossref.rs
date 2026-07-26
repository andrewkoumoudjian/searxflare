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

pub struct CrossrefEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "crossref",
    display_name: "Crossref",
    categories: &["academic"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["api.crossref.org"],
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
    weight: 1.3,
    parser_version: "crossref-works-json-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 900,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Crossref time-range filtering is not exposed by this adapter",
        ));
    }

    let page = query.page_number();
    if page > 500 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "Crossref supports at most 500 pages in this adapter",
        ));
    }

    let rows = u32::from(query.limit);
    let offset = page.saturating_sub(1).saturating_mul(rows);
    let mut url = Url::parse("https://api.crossref.org/works").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("query.bibliographic", &query.text)
        .append_pair("rows", &rows.to_string())
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
        accepted_content_types: &["application/json", "application/vnd.crossref.unixref+xml"],
    })
}

fn first_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn publication_date(item: &Value) -> Option<String> {
    let parts = ["published", "published-online", "published-print", "issued"]
        .iter()
        .find_map(|field| item.pointer(&format!("/{field}/date-parts/0")))?
        .as_array()?;
    let year = parts.first()?.as_i64()?;
    let month = parts.get(1).and_then(Value::as_i64);
    let day = parts.get(2).and_then(Value::as_i64);

    match (month, day) {
        (Some(month), Some(day)) if (1..=12).contains(&month) && (1..=31).contains(&day) => {
            Some(format!("{year:04}-{month:02}-{day:02}"))
        }
        (Some(month), _) if (1..=12).contains(&month) => Some(format!("{year:04}-{month:02}")),
        _ => Some(format!("{year:04}")),
    }
}

fn author_names(item: &Value) -> Vec<String> {
    item.get("author")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|author| {
            let given = author
                .get("given")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let family = author
                .get("family")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let name = format!("{given} {family}").trim().to_owned();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

fn result_url(item: &Value) -> Option<String> {
    let direct = item
        .get("URL")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let candidate = direct.map(str::to_owned).or_else(|| {
        item.get("DOI")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|doi| format!("https://doi.org/{doi}"))
    })?;
    Url::parse(&candidate)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|_| candidate)
}

fn classify_api_error(root: &Value) -> EngineFailure {
    let message = root
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Crossref returned an unsuccessful API response");
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("rate") && lower.contains("limit") {
        FailureKind::EngineRateLimited
    } else {
        FailureKind::EngineParseFailed
    };
    EngineFailure::new(DESCRIPTOR.id, kind, message)
}

fn parse_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if root.get("status").and_then(Value::as_str) != Some("ok") {
        return Err(classify_api_error(&root));
    }

    let items = root
        .pointer("/message/items")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Crossref response is missing message.items",
            )
        })?;

    let mut results = Vec::new();
    for item in items {
        let Some(title) = first_string(item.get("title")) else {
            continue;
        };
        let Some(url) = result_url(item) else {
            continue;
        };
        let authors = author_names(item);
        let container_title = first_string(item.get("container-title"));
        let abstract_text = item
            .get("abstract")
            .and_then(Value::as_str)
            .map(strip_markup)
            .filter(|value| !value.is_empty())
            .or_else(|| container_title.clone())
            .unwrap_or_default();

        let mut metadata = Map::new();
        metadata.insert("authors".into(), json!(authors));
        if let Some(doi) = item.get("DOI").and_then(Value::as_str) {
            metadata.insert("doi".into(), Value::String(doi.into()));
        }
        if let Some(container_title) = container_title {
            metadata.insert("container_title".into(), Value::String(container_title));
        }
        for field in ["type", "publisher"] {
            if let Some(value) = item.get(field).and_then(Value::as_str) {
                metadata.insert(field.into(), Value::String(value.into()));
            }
        }
        if let Some(value) = item.get("is-referenced-by-count").and_then(Value::as_u64) {
            metadata.insert("citation_count".into(), json!(value));
        }
        if let Some(subjects) = item.get("subject").and_then(Value::as_array) {
            metadata.insert("subjects".into(), Value::Array(subjects.clone()));
        }

        results.push(ProviderResult {
            url,
            title: strip_markup(&title),
            content: abstract_text,
            published_at: publication_date(item),
            thumbnail: None,
            category: "academic".into(),
            metadata,
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for CrossrefEngine {
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
            text: "cloudflare rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 10,
            locale: None,
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::RrfV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn builds_bounded_paged_request() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(parameters.get("rows").map(String::as_str), Some("10"));
        assert_eq!(parameters.get("offset").map(String::as_str), Some("10"));
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
    fn parses_normal_empty_and_failure_fixtures() {
        let normal = parse_results(include_bytes!(
            "../../../fixtures/engines/crossref/normal.json"
        ))
        .unwrap();
        assert_eq!(normal.len(), 1);
        assert_eq!(normal[0].title, "Cloudflare Workers research");
        assert_eq!(normal[0].published_at.as_deref(), Some("2026-07-01"));
        assert!(!normal[0].content.contains('<'));

        let empty = parse_results(include_bytes!(
            "../../../fixtures/engines/crossref/empty.json"
        ))
        .unwrap();
        assert!(empty.is_empty());

        assert_eq!(
            parse_results(include_bytes!(
                "../../../fixtures/engines/crossref/rate-limited.json"
            ))
            .unwrap_err()
            .kind,
            FailureKind::EngineRateLimited
        );
        assert!(parse_results(include_bytes!(
            "../../../fixtures/engines/crossref/changed-schema.json"
        ))
        .is_err());
    }
}
