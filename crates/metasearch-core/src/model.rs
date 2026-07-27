use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub const DEFAULT_ENGINE_TIMEOUT_MS: u32 = 2_500;
pub const HTML_ENGINE_TIMEOUT_MS: u32 = 3_500;
pub const DEFAULT_OVERALL_TIMEOUT_MS: u32 = 5_000;
pub const MAX_OVERALL_TIMEOUT_MS: u32 = 8_000;
pub const DEFAULT_MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_MAX_STEPS: u8 = 3;
pub const DEFAULT_MAX_REDIRECTS: u8 = 2;
pub const DEFAULT_RESULT_LIMIT: u8 = 10;
pub const MAX_RESULT_LIMIT: u8 = 20;
pub const DEFAULT_ENGINE_COUNT: usize = 3;
pub const MAX_ENGINE_COUNT: usize = 5;
pub const MAX_CATEGORY_COUNT: usize = 3;
pub const MAX_TOTAL_UPSTREAM_REQUESTS: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SafeSearch {
    Off,
    #[default]
    Moderate,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RankingStrategy {
    #[default]
    QueryAwareV1,
    RrfV1,
    SearxCompatV1,
}

impl RankingStrategy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QueryAwareV1 => "query-aware-v1",
            Self::RrfV1 => "rrf-v1",
            Self::SearxCompatV1 => "searx-compat-v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Atom,
    Json,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMaturity {
    Experimental,
    Beta,
    Stable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct EngineCapabilities {
    pub paging: bool,
    pub locale: bool,
    pub country: bool,
    pub safe_search: bool,
    pub time_range: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatePolicy {
    Stateless,
    KvSnapshot,
    DurableCoordinator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CachePolicy {
    pub response_ttl_seconds: u32,
    pub negative_ttl_seconds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BotAuthPolicy {
    Disabled,
    Optional,
    Required,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct EngineDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub categories: &'static [&'static str],
    pub source_kind: SourceKind,
    pub maturity: EngineMaturity,
    pub allowed_hosts: &'static [&'static str],
    pub capabilities: EngineCapabilities,
    pub timeout_ms: u32,
    pub max_body_bytes: usize,
    pub max_steps: u8,
    pub max_redirects: u8,
    pub weight: f64,
    pub parser_version: &'static str,
    pub default_enabled: bool,
    pub allow_http: bool,
    pub state_policy: StatePolicy,
    pub cache_policy: CachePolicy,
    pub bot_auth_policy: BotAuthPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedQuery {
    pub text: String,
    pub engines: Vec<String>,
    pub categories: Vec<String>,
    pub page: Option<u32>,
    pub cursor: Option<String>,
    pub limit: u8,
    pub locale: Option<String>,
    pub country: Option<String>,
    pub safe_search: SafeSearch,
    pub time_range: Option<TimeRange>,
    pub ranking: RankingStrategy,
    pub timeout_ms: u32,
}

impl NormalizedQuery {
    pub fn page_number(&self) -> u32 {
        self.page.unwrap_or(1)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderResult {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub category: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
    pub engine_id: String,
    pub position: u32,
    pub engine_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineContribution {
    pub engine_id: String,
    pub position: u32,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedResult {
    pub url: String,
    pub canonical_url: String,
    pub title: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub category: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
    #[serde(default)]
    pub provider_metadata: BTreeMap<String, Map<String, Value>>,
    pub engines: Vec<String>,
    pub positions: BTreeMap<String, u32>,
    #[serde(default, skip_serializing)]
    pub contributions: Vec<EngineContribution>,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineOutput {
    pub results: Vec<ProviderResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub upstream_requests: u8,
    #[serde(default)]
    pub response_bytes: usize,
    #[serde(default)]
    pub parse_ms: u64,
    #[serde(default)]
    pub redirect_count: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCoordinatorCommand {
    AcquireRefreshLease {
        now_ms: u64,
        lease_ms: u32,
    },
    RecordSuccess {
        now_ms: u64,
    },
    RecordFailure {
        now_ms: u64,
        failure_kind: String,
        cooldown_ms: u32,
    },
    Snapshot {
        now_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderCoordinatorSnapshot {
    pub refresh_lease_acquired: bool,
    pub lease_expires_at_ms: Option<u64>,
    pub cooldown_expires_at_ms: Option<u64>,
    pub consecutive_failures: u32,
    pub last_failure_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStatus {
    Hit,
    Miss,
    Bypass,
    NegativeHit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineExecutionReport {
    pub engine_id: String,
    pub duration_ms: u64,
    pub cache_status: CacheStatus,
    pub result_count: usize,
    #[serde(default)]
    pub response_bytes: usize,
    #[serde(default)]
    pub parse_ms: u64,
    #[serde(default)]
    pub redirect_count: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parser_version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    expires_at_ms: u64,
}

impl Deadline {
    pub const fn at(expires_at_ms: u64) -> Self {
        Self { expires_at_ms }
    }

    pub fn from_now(now_ms: u64, timeout_ms: u32) -> Self {
        Self::at(now_ms.saturating_add(u64::from(timeout_ms)))
    }

    pub const fn expires_at_ms(self) -> u64 {
        self.expires_at_ms
    }

    pub fn remaining_ms(self, now_ms: u64) -> u64 {
        self.expires_at_ms.saturating_sub(now_ms)
    }

    pub fn is_expired(self, now_ms: u64) -> bool {
        self.remaining_ms(now_ms) == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMethod {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub struct EngineRequest {
    pub method: EngineMethod,
    pub url: Url,
    pub headers: BTreeMap<String, String>,
    pub cookies: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub accepted_content_types: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct BoundedResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub final_url: Url,
    pub redirect_count: u8,
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_result_round_trips_without_internal_contributions() {
        let result = NormalizedResult {
            url: "https://example.com/result".into(),
            canonical_url: "https://example.com/result".into(),
            title: "Example result".into(),
            content: "Result content".into(),
            published_at: None,
            thumbnail: None,
            category: "general".into(),
            metadata: Map::new(),
            provider_metadata: BTreeMap::from([("example".into(), Map::new())]),
            engines: vec!["example".into()],
            positions: BTreeMap::from([("example".into(), 1)]),
            contributions: vec![EngineContribution {
                engine_id: "example".into(),
                position: 1,
                weight: 1.0,
            }],
            score: 1.0,
        };

        let encoded = serde_json::to_value(&result).unwrap();
        assert!(encoded.get("contributions").is_none());
        assert!(encoded.get("provider_metadata").is_some());

        let decoded: NormalizedResult = serde_json::from_value(encoded).unwrap();
        assert!(decoded.contributions.is_empty());
        assert_eq!(decoded.url, result.url);
        assert_eq!(decoded.engines, result.engines);
        assert!(decoded.provider_metadata.contains_key("example"));
    }
}
