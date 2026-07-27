use crate::{NormalizedResult, RankingStrategy};
use entropy_map::{Set as EntropySet, DEFAULT_GAMMA};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use unicode_normalization::UnicodeNormalization;
use wildcard::Wildcard;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankingError {
    InvalidPosition,
    InvalidWeight,
}

impl Display for RankingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPosition => formatter.write_str("provider positions must be positive"),
            Self::InvalidWeight => {
                formatter.write_str("engine weights must be finite and non-negative")
            }
        }
    }
}

impl std::error::Error for RankingError {}

fn rrf_score(result: &NormalizedResult) -> Result<f64, RankingError> {
    let mut score = 0.0;
    for contribution in &result.contributions {
        if contribution.position == 0 {
            return Err(RankingError::InvalidPosition);
        }
        if !contribution.weight.is_finite() || contribution.weight < 0.0 {
            return Err(RankingError::InvalidWeight);
        }
        score += contribution.weight / (60.0 + f64::from(contribution.position));
    }
    score += 0.10 * (1.0 + result.engines.len() as f64).ln();
    Ok(score)
}

fn searx_compat_score(result: &NormalizedResult) -> Result<f64, RankingError> {
    let mut weight_product = 1.0;
    for contribution in &result.contributions {
        if contribution.position == 0 {
            return Err(RankingError::InvalidPosition);
        }
        if !contribution.weight.is_finite() || contribution.weight < 0.0 {
            return Err(RankingError::InvalidWeight);
        }
        weight_product *= contribution.weight;
    }
    weight_product *= result.contributions.len() as f64;
    Ok(result
        .contributions
        .iter()
        .map(|contribution| weight_product / f64::from(contribution.position))
        .sum())
}

fn normalize_lexical(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut previous_space = true;
    for character in value.nfkc().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            normalized.push(character);
            previous_space = false;
        } else if !previous_space {
            normalized.push(' ');
            previous_space = true;
        }
    }
    normalized.trim().to_owned()
}

fn tokens(value: &str) -> BTreeSet<String> {
    normalize_lexical(value)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

fn wildcard_pattern(token: &str) -> Vec<char> {
    let mut pattern = Vec::with_capacity(token.chars().count() + 1);
    for character in token.chars() {
        if matches!(character, '*' | '?' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('*');
    pattern
}

fn token_coverage(query_tokens: &BTreeSet<String>, haystack: &str) -> f64 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let haystack_tokens = tokens(haystack);
    let haystack_index: Option<EntropySet<String>> =
        EntropySet::from_iter_with_params(haystack_tokens.iter().cloned(), DEFAULT_GAMMA).ok();
    let covered = query_tokens
        .iter()
        .filter(|token| {
            if haystack_index
                .as_ref()
                .is_some_and(|index| index.contains(token.as_str()))
            {
                return true;
            }
            if token.chars().count() < 4 {
                return false;
            }
            let pattern = wildcard_pattern(token);
            let Ok(wildcard) = Wildcard::new(&pattern) else {
                return false;
            };
            haystack_tokens.iter().any(|candidate| {
                let candidate = candidate.chars().collect::<Vec<_>>();
                wildcard.is_match(&candidate)
            })
        })
        .count();
    covered as f64 / query_tokens.len() as f64
}

fn publication_year(value: Option<&str>) -> Option<i32> {
    let value = value?;
    let prefix = value.get(..4)?;
    prefix
        .parse()
        .ok()
        .filter(|year| (1000..=9999).contains(year))
}

fn freshness_bonus(result: &NormalizedResult, newest_year: Option<i32>) -> f64 {
    let Some(newest_year) = newest_year else {
        return 0.0;
    };
    let Some(year) = publication_year(result.published_at.as_deref()) else {
        return 0.0;
    };
    match newest_year.saturating_sub(year) {
        0 => 0.05,
        1 => 0.04,
        2..=5 => 0.02,
        _ => 0.0,
    }
}

fn query_aware_score(
    result: &NormalizedResult,
    query: &str,
    newest_year: Option<i32>,
) -> Result<f64, RankingError> {
    let base = rrf_score(result)?;
    let normalized_query = normalize_lexical(query);
    let query_tokens = tokens(query);
    if query_tokens.is_empty() {
        return Ok(base);
    }

    let normalized_title = normalize_lexical(&result.title);
    let normalized_content = normalize_lexical(&result.content);
    let mut score = base;

    if !normalized_query.is_empty() && normalized_title.contains(&normalized_query) {
        score += 2.0;
    }
    if !normalized_query.is_empty() && normalized_content.contains(&normalized_query) {
        score += 0.75;
    }

    score += 1.5 * token_coverage(&query_tokens, &result.title);
    score += 0.60 * token_coverage(&query_tokens, &result.content);
    score += 0.25 * token_coverage(&query_tokens, &result.canonical_url);
    score += freshness_bonus(result, newest_year);
    Ok(score)
}

pub fn rank_results(
    mut results: Vec<NormalizedResult>,
    strategy: RankingStrategy,
    original_query: &str,
) -> Result<Vec<NormalizedResult>, RankingError> {
    let newest_year = results
        .iter()
        .filter_map(|result| publication_year(result.published_at.as_deref()))
        .max();

    for result in &mut results {
        result.score = match strategy {
            RankingStrategy::QueryAwareV1 => {
                query_aware_score(result, original_query, newest_year)?
            }
            RankingStrategy::RrfV1 => rrf_score(result)?,
            RankingStrategy::SearxCompatV1 => searx_compat_score(result)?,
        };
    }
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.canonical_url.cmp(&right.canonical_url))
    });
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineContribution;
    use serde_json::Map;
    use std::collections::BTreeMap;

    fn result(url: &str, positions: &[(&str, u32, f64)]) -> NormalizedResult {
        NormalizedResult {
            url: url.into(),
            canonical_url: url.into(),
            title: url.into(),
            content: String::new(),
            published_at: None,
            thumbnail: None,
            category: "general".into(),
            metadata: Map::new(),
            provider_metadata: BTreeMap::new(),
            engines: positions
                .iter()
                .map(|(id, _, _)| (*id).to_owned())
                .collect(),
            positions: positions
                .iter()
                .map(|(id, position, _)| ((*id).to_owned(), *position))
                .collect::<BTreeMap<_, _>>(),
            contributions: positions
                .iter()
                .map(|(id, position, weight)| EngineContribution {
                    engine_id: (*id).to_owned(),
                    position: *position,
                    weight: *weight,
                })
                .collect(),
            score: 0.0,
        }
    }

    #[test]
    fn query_aware_ranking_prefers_original_query_relevance() {
        let mut relevant = result("https://example.com/relevant", &[("a", 4, 1.0)]);
        relevant.title = "Cloudflare Rust Workers guide".into();
        relevant.content = "Build Rust applications on Cloudflare Workers.".into();
        relevant.published_at = Some("2026-07-01T00:00:00Z".into());

        let mut provider_first = result("https://example.com/first", &[("a", 1, 1.0)]);
        provider_first.title = "Unrelated result".into();
        provider_first.content = "No matching terms.".into();
        provider_first.published_at = Some("2020-01-01T00:00:00Z".into());

        let ranked = rank_results(
            vec![provider_first, relevant],
            RankingStrategy::QueryAwareV1,
            "!web cloudflare rust",
        )
        .unwrap();
        assert_eq!(ranked[0].canonical_url, "https://example.com/relevant");
    }

    #[test]
    fn query_aware_falls_back_to_rrf_without_tokens() {
        let ranked = rank_results(
            vec![
                result("https://single.example/", &[("a", 1, 1.0)]),
                result(
                    "https://consensus.example/",
                    &[("a", 3, 1.0), ("b", 4, 1.0)],
                ),
            ],
            RankingStrategy::QueryAwareV1,
            "!!!",
        )
        .unwrap();
        assert_eq!(ranked[0].canonical_url, "https://consensus.example/");
    }

    #[test]
    fn query_aware_matches_normalized_token_prefixes() {
        let mut relevant = result("https://example.com/relevant", &[("a", 3, 1.0)]);
        relevant.title = "Distributed systems for the edge".into();
        let mut unrelated = result("https://example.com/unrelated", &[("a", 1, 1.0)]);
        unrelated.title = "A cooking reference".into();

        let ranked = rank_results(
            vec![unrelated, relevant],
            RankingStrategy::QueryAwareV1,
            "distribution edge",
        )
        .unwrap();
        assert_eq!(ranked[0].canonical_url, "https://example.com/relevant");
    }

    #[test]
    fn rrf_rewards_consensus() {
        let ranked = rank_results(
            vec![
                result("https://single.example/", &[("a", 1, 1.0)]),
                result(
                    "https://consensus.example/",
                    &[("a", 3, 1.0), ("b", 4, 1.0)],
                ),
            ],
            RankingStrategy::RrfV1,
            "rust",
        )
        .unwrap();
        assert_eq!(ranked[0].canonical_url, "https://consensus.example/");
    }

    #[test]
    fn tie_breaks_by_canonical_url() {
        let ranked = rank_results(
            vec![
                result("https://b.example/", &[("a", 1, 1.0)]),
                result("https://a.example/", &[("a", 1, 1.0)]),
            ],
            RankingStrategy::RrfV1,
            "rust",
        )
        .unwrap();
        assert_eq!(ranked[0].canonical_url, "https://a.example/");
    }

    #[test]
    fn searx_compat_matches_documented_formula() {
        let ranked = rank_results(
            vec![result(
                "https://example.com/",
                &[("a", 1, 2.0), ("b", 2, 0.5)],
            )],
            RankingStrategy::SearxCompatV1,
            "rust",
        )
        .unwrap();
        assert!((ranked[0].score - 3.0).abs() < f64::EPSILON);
    }
}
