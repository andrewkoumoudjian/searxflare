mod cache;
mod canonicalize;
mod cursor;
mod dedup;
mod error;
mod model;
mod query;
mod ranking;
mod traits;

pub use cache::{build_cache_key, CacheKeyInput, API_VERSION, ENGINE_REGISTRY_VERSION, RANKING_VERSION};
pub use canonicalize::{canonicalize_url, CanonicalizationError};
pub use cursor::{CursorError, CursorPayload, CursorSigner};
pub use dedup::{deduplicate, normalize_provider_result};
pub use error::{EngineFailure, FailureKind};
pub use model::*;
pub use query::{normalize_query, QueryNormalizationError};
pub use ranking::{rank_results, RankingError};
pub use traits::{EngineContext, EngineHttpClient, EngineState, SearchEngine};
