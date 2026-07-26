use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use metasearch_parsers::{parse_arxiv_atom, ParserError};
use serde_json::{json, Map};
use std::collections::BTreeMap;
use url::Url;

pub struct ArxivEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "arxiv",
    display_name: "arXiv",
    categories: &["academic"],
    source_kind: SourceKind::Atom,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["export.arxiv.org"],
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
    weight: 1.25,
    parser_version: "arxiv-atom-v2",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 1_800,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn arxiv_search_query(text: &str) -> Result<String, EngineFailure> {
    let terms = text
        .split_whitespace()
        .filter_map(|term| {
            let cleaned: String = term
                .chars()
                .map(|character| match character {
                    '"' | '\\' => ' ',
                    other => other,
                })
                .collect();
            let cleaned = cleaned.trim();
            (!cleaned.is_empty()).then(|| format!("all:\"{cleaned}\""))
        })
        .collect::<Vec<_>>();

    if terms.is_empty() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::InvalidRequest,
            "query does not contain an arXiv-searchable term",
        ));
    }
    Ok(terms.join(" AND "))
}

fn build_request(query: &NormalizedQuery) -> Result<EngineRequest, EngineFailure> {
    let page_size = u32::from(query.limit);
    let start = query
        .page_number()
        .saturating_sub(1)
        .saturating_mul(page_size);
    let mut url = Url::parse("https://export.arxiv.org/api/query").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("search_query", &arxiv_search_query(&query.text)?)
        .append_pair("start", &start.to_string())
        .append_pair("max_results", &page_size.to_string());

    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([
            (
                "accept".into(),
                "application/atom+xml, application/xml;q=0.9".into(),
            ),
            (
                "user-agent".into(),
                "searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)".into(),
            ),
        ]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/atom+xml", "application/xml", "text/xml"],
    })
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for ArxivEngine {
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
        let records = parse_arxiv_atom(&response.body).map_err(|error| {
            let kind = match &error {
                ParserError::ProviderError(_) => FailureKind::InvalidRequest,
                _ => FailureKind::EngineParseFailed,
            };
            EngineFailure::new(DESCRIPTOR.id, kind, error.to_string())
        })?;
        let results = records
            .into_iter()
            .enumerate()
            .map(|(index, record)| {
                let mut metadata = Map::new();
                metadata.insert("authors".into(), json!(record.authors));
                metadata.insert("pdf_url".into(), json!(record.pdf_url));
                metadata.insert("doi".into(), json!(record.doi));
                metadata.insert("journal_reference".into(), json!(record.journal_reference));
                metadata.insert("categories".into(), json!(record.categories));
                metadata.insert("comments".into(), json!(record.comments));
                ProviderResult {
                    url: record.canonical_url,
                    title: record.title,
                    content: record.abstract_text,
                    published_at: Some(record.published_at),
                    thumbnail: None,
                    category: "academic".into(),
                    metadata,
                    engine_id: DESCRIPTOR.id.into(),
                    position: (index + 1) as u32,
                    engine_weight: DESCRIPTOR.weight,
                }
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
    use metasearch_core::{RankingStrategy, SafeSearch};

    fn query() -> NormalizedQuery {
        NormalizedQuery {
            text: "cloudflare rust".into(),
            engines: Vec::new(),
            categories: Vec::new(),
            page: Some(2),
            cursor: None,
            limit: 7,
            locale: None,
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            ranking: RankingStrategy::RrfV1,
            timeout_ms: 5_000,
        }
    }

    #[test]
    fn qualifies_each_query_term_and_respects_page_size() {
        let request = build_request(&query()).unwrap();
        let parameters: BTreeMap<_, _> = request.url.query_pairs().into_owned().collect();
        assert_eq!(
            parameters.get("search_query").map(String::as_str),
            Some("all:\"cloudflare\" AND all:\"rust\"")
        );
        assert_eq!(parameters.get("start").map(String::as_str), Some("7"));
        assert_eq!(
            parameters.get("max_results").map(String::as_str),
            Some("7")
        );
    }
}
