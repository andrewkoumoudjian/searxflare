use crate::{RankingStrategy, SafeSearch, TimeRange};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const API_VERSION: &str = "v1";
pub const ENGINE_REGISTRY_VERSION: &str = "2026-07-26.3";
pub const RANKING_VERSION: &str = "2026-07-26.1";

#[derive(Debug, Clone, Serialize)]
pub struct CacheKeyInput<'a> {
    pub normalized_query: &'a str,
    pub engine_ids: &'a [String],
    pub categories: &'a [String],
    pub page: Option<u32>,
    pub cursor_hash: Option<&'a str>,
    pub limit: u8,
    pub locale: Option<&'a str>,
    pub country: Option<&'a str>,
    pub safe_search: SafeSearch,
    pub time_range: Option<TimeRange>,
    pub parser_versions: &'a [String],
    pub ranking: RankingStrategy,
}

pub fn build_cache_key(input: &CacheKeyInput<'_>) -> String {
    let mut engine_ids = input.engine_ids.to_vec();
    engine_ids.sort();
    let mut categories = input.categories.to_vec();
    categories.sort();
    let mut parser_versions = input.parser_versions.to_vec();
    parser_versions.sort();

    let material = serde_json::json!({
        "api_version": API_VERSION,
        "query": input.normalized_query,
        "engines": engine_ids,
        "categories": categories,
        "page": input.page,
        "cursor_hash": input.cursor_hash,
        "limit": input.limit,
        "locale": input.locale,
        "country": input.country,
        "safe_search": input.safe_search,
        "time_range": input.time_range,
        "registry": ENGINE_REGISTRY_VERSION,
        "parsers": parser_versions,
        "ranking": input.ranking,
        "ranking_version": RANKING_VERSION,
    });
    let digest =
        Sha256::digest(serde_json::to_vec(&material).expect("cache key material serializes"));
    format!("https://cache.searxflare.invalid/{API_VERSION}/search/{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_order_does_not_change_key() {
        let left_engines = vec!["wikipedia".into(), "arxiv".into()];
        let right_engines = vec!["arxiv".into(), "wikipedia".into()];
        let parser_versions = vec!["a:1".into(), "b:2".into()];
        let left = CacheKeyInput {
            normalized_query: "rust",
            engine_ids: &left_engines,
            categories: &[],
            page: Some(1),
            cursor_hash: None,
            limit: 10,
            locale: Some("en-US"),
            country: None,
            safe_search: SafeSearch::Moderate,
            time_range: None,
            parser_versions: &parser_versions,
            ranking: RankingStrategy::RrfV1,
        };
        let right = CacheKeyInput {
            engine_ids: &right_engines,
            ..left.clone()
        };

        assert_eq!(build_cache_key(&left), build_cache_key(&right));
    }
}
