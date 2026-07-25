use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use metasearch_parsers::parse_arxiv_atom;
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
    parser_version: "arxiv-atom-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 1_800,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

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
        let start = query.page_number().saturating_sub(1).saturating_mul(10);
        let mut url = Url::parse("https://export.arxiv.org/api/query").map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
        url.query_pairs_mut()
            .append_pair("search_query", &format!("all:{}", query.text))
            .append_pair("start", &start.to_string())
            .append_pair("max_results", "10");
        let request = EngineRequest {
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
        };
        let response = context
            .http
            .send(&DESCRIPTOR, request, context.deadline)
            .await?;
        let records = parse_arxiv_atom(&response.body).map_err(|error| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                error.to_string(),
            )
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
