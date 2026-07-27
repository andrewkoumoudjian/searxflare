use crate::text::strip_markup;
use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct PubMedEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "pubmed",
    display_name: "PubMed",
    categories: &["academic"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["eutils.ncbi.nlm.nih.gov"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: false,
        country: false,
        safe_search: false,
        time_range: false,
    },
    timeout_ms: DEFAULT_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: 2,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.4,
    parser_version: "pubmed-eutils-json-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 900,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn common_headers() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("accept".into(), "application/json".into()),
        (
            "user-agent".into(),
            "Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
        ),
    ])
}

fn build_search_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    if query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "PubMed time-range filtering is not exposed by this adapter",
        ));
    }
    let page = query.page_number();
    if page > 500 {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "PubMed supports at most 500 pages in this adapter",
        ));
    }

    let limit = u32::from(query.limit);
    let start = page.saturating_sub(1).saturating_mul(limit);
    let mut url = Url::parse("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi")
        .map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
    url.query_pairs_mut()
        .append_pair("db", "pubmed")
        .append_pair("term", &query.text)
        .append_pair("retmode", "json")
        .append_pair("retstart", &start.to_string())
        .append_pair("retmax", &limit.to_string())
        .append_pair("sort", "relevance");

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: common_headers(),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json", "text/json"],
    })
}

fn build_summary_request(ids: &[String]) -> Result<EngineRequest, EngineFailure> {
    let mut url = Url::parse("https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi")
        .map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
    url.query_pairs_mut()
        .append_pair("db", "pubmed")
        .append_pair("id", &ids.join(","))
        .append_pair("retmode", "json")
        .append_pair("version", "2.0");

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: common_headers(),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json", "text/json"],
    })
}

fn classify_api_error(root: &Value, fallback: &str) -> EngineFailure {
    let message = root
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| root.pointer("/esearchresult/error").and_then(Value::as_str))
        .unwrap_or(fallback);
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("rate") || lower.contains("too many") {
        FailureKind::EngineRateLimited
    } else {
        FailureKind::EngineParseFailed
    };
    EngineFailure::new(DESCRIPTOR.id, kind, message)
}

fn parse_search_ids(body: &[u8]) -> Result<Vec<String>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if root.get("error").is_some() || root.pointer("/esearchresult/error").is_some() {
        return Err(classify_api_error(
            &root,
            "PubMed ESearch returned an error",
        ));
    }
    let ids = root
        .pointer("/esearchresult/idlist")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "PubMed ESearch response is missing esearchresult.idlist",
            )
        })?;
    Ok(ids
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect())
}

fn author_names(item: &Value) -> Vec<String> {
    item.get("authors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|author| author.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn article_id(item: &Value, id_type: &str) -> Option<String> {
    item.get("articleids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|value| value.get("idtype").and_then(Value::as_str) == Some(id_type))
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn publication_date(item: &Value) -> Option<String> {
    item.get("sortpubdate")
        .and_then(Value::as_str)
        .map(str::trim)
        .and_then(|value| value.get(..10))
        .map(|value| value.replace('/', "-"))
        .or_else(|| {
            item.get("pubdate")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

fn parse_summary_results(body: &[u8]) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    if root.get("error").is_some() {
        return Err(classify_api_error(
            &root,
            "PubMed ESummary returned an error",
        ));
    }
    let result = root
        .get("result")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "PubMed ESummary response is missing result",
            )
        })?;
    let uids = result
        .get("uids")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "PubMed ESummary response is missing result.uids",
            )
        })?;

    let mut results = Vec::new();
    for uid in uids.iter().filter_map(Value::as_str) {
        let Some(item) = result.get(uid) else {
            continue;
        };
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .map(strip_markup)
            .filter(|value| !value.is_empty());
        let Some(title) = title else {
            continue;
        };
        let authors = author_names(item);
        let journal = item
            .get("fulljournalname")
            .and_then(Value::as_str)
            .or_else(|| item.get("source").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let publication_types: Vec<String> = item
            .get("pubtype")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();

        let mut metadata = Map::new();
        metadata.insert("pmid".into(), Value::String(uid.into()));
        metadata.insert("authors".into(), json!(authors));
        metadata.insert("publication_types".into(), json!(publication_types));
        if let Some(doi) = article_id(item, "doi") {
            metadata.insert("doi".into(), Value::String(doi));
        }
        if let Some(pmcid) = article_id(item, "pmc") {
            metadata.insert("pmcid".into(), Value::String(pmcid));
        }
        if let Some(journal) = &journal {
            metadata.insert("journal".into(), Value::String(journal.clone()));
        }

        results.push(ProviderResult {
            url: format!("https://pubmed.ncbi.nlm.nih.gov/{uid}/"),
            title,
            content: journal.unwrap_or_default(),
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
impl SearchEngine for PubMedEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let search_response = context
            .http
            .send(&DESCRIPTOR, build_search_request(query)?, context.deadline)
            .await?;
        let ids = parse_search_ids(&search_response.body)?;
        if ids.is_empty() {
            return Ok(EngineOutput {
                results: Vec::new(),
                next_cursor: None,
                upstream_requests: 1,
                response_bytes: search_response.body.len(),
                parse_ms: 0,
                redirect_count: search_response.redirect_count,
            });
        }

        let summary_response = context
            .http
            .send(&DESCRIPTOR, build_summary_request(&ids)?, context.deadline)
            .await?;
        let results = parse_summary_results(&summary_response.body)?;
        Ok(EngineOutput {
            results,
            next_cursor: None,
            upstream_requests: 2,
            response_bytes: search_response
                .body
                .len()
                .saturating_add(summary_response.body.len()),
            parse_ms: 0,
            redirect_count: search_response
                .redirect_count
                .saturating_add(summary_response.redirect_count),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metasearch_core::{RankingStrategy, SafeSearch, TimeRange};

    fn query() -> NormalizedQuery {
        NormalizedQuery {
            text: "food safety".into(),
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
    fn builds_two_step_reference_requests() {
        let search = build_search_request(&query()).unwrap();
        let search_parameters: BTreeMap<_, _> = search.url.query_pairs().into_owned().collect();
        assert_eq!(
            search_parameters.get("retstart").map(String::as_str),
            Some("10")
        );

        let summary = build_summary_request(&["1".into(), "2".into()]).unwrap();
        let summary_parameters: BTreeMap<_, _> = summary.url.query_pairs().into_owned().collect();
        assert_eq!(
            summary_parameters.get("id").map(String::as_str),
            Some("1,2")
        );
    }

    #[test]
    fn rejects_unsupported_time_range() {
        let mut value = query();
        value.time_range = Some(TimeRange::Month);
        assert_eq!(
            build_search_request(&value).unwrap_err().kind,
            FailureKind::UnsupportedCapability
        );
    }

    #[test]
    fn parses_search_and_summary_fixtures() {
        let ids = parse_search_ids(include_bytes!(
            "../../../fixtures/engines/pubmed/search-normal.json"
        ))
        .unwrap();
        assert_eq!(ids, vec!["12345678".to_string()]);

        let empty = parse_search_ids(include_bytes!(
            "../../../fixtures/engines/pubmed/search-empty.json"
        ))
        .unwrap();
        assert!(empty.is_empty());

        let results = parse_summary_results(include_bytes!(
            "../../../fixtures/engines/pubmed/summary-normal.json"
        ))
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Food safety and Worker systems");
        assert_eq!(results[0].published_at.as_deref(), Some("2026-06-15"));

        assert!(parse_search_ids(include_bytes!(
            "../../../fixtures/engines/pubmed/changed-schema.json"
        ))
        .is_err());
        assert_eq!(
            parse_search_ids(include_bytes!(
                "../../../fixtures/engines/pubmed/rate-limited.json"
            ))
            .unwrap_err()
            .kind,
            FailureKind::EngineRateLimited
        );
    }
}
