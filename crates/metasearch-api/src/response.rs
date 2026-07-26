use metasearch_core::{EngineDescriptor, EngineExecutionReport, NormalizedResult, RankingStrategy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub request_id: String,
    pub query: String,
    pub provider_query: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bangs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_engines: Vec<String>,
    pub ranking: RankingStrategy,
    pub partial: bool,
    pub cached: bool,
    pub result_count: usize,
    pub results: Vec<NormalizedResult>,
    pub engines: Vec<EngineExecutionReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineDescriptorResponse {
    #[serde(flatten)]
    pub descriptor: &'static EngineDescriptor,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineCatalogueResponse {
    pub registry_version: &'static str,
    pub engines: Vec<EngineDescriptorResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearxCompatResponse {
    pub query: String,
    pub number_of_results: usize,
    pub results: Vec<NormalizedResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresponsive_engines: Vec<(String, String)>,
}
