use crate::{NormalizedResult, RankingStrategy};
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankingError {
    InvalidPosition,
    InvalidWeight,
}

impl Display for RankingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPosition => formatter.write_str("provider positions must be positive"),
            Self::InvalidWeight => formatter.write_str("engine weights must be finite and non-negative"),
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

pub fn rank_results(
    mut results: Vec<NormalizedResult>,
    strategy: RankingStrategy,
) -> Result<Vec<NormalizedResult>, RankingError> {
    for result in &mut results {
        result.score = match strategy {
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
            engines: positions.iter().map(|(id, _, _)| (*id).to_owned()).collect(),
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
    fn rrf_rewards_consensus() {
        let ranked = rank_results(
            vec![
                result("https://single.example/", &[("a", 1, 1.0)]),
                result("https://consensus.example/", &[("a", 3, 1.0), ("b", 4, 1.0)]),
            ],
            RankingStrategy::RrfV1,
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
        )
        .unwrap();
        assert_eq!(ranked[0].canonical_url, "https://a.example/");
    }

    #[test]
    fn searx_compat_matches_documented_formula() {
        let ranked = rank_results(
            vec![result("https://example.com/", &[("a", 1, 2.0), ("b", 2, 0.5)])],
            RankingStrategy::SearxCompatV1,
        )
        .unwrap();
        assert!((ranked[0].score - 3.0).abs() < f64::EPSILON);
    }
}
