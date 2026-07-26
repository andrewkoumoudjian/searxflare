use crate::{
    canonicalize_url, CanonicalizationError, EngineContribution, NormalizedResult, ProviderResult,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn normalize_provider_result(
    result: ProviderResult,
) -> Result<NormalizedResult, CanonicalizationError> {
    let canonical_url = canonicalize_url(&result.url)?;
    let mut positions = BTreeMap::new();
    positions.insert(result.engine_id.clone(), result.position);
    let provider_metadata = BTreeMap::from([(result.engine_id.clone(), result.metadata.clone())]);
    Ok(NormalizedResult {
        url: result.url,
        canonical_url,
        title: result.title.trim().to_owned(),
        content: result.content.trim().to_owned(),
        published_at: result.published_at,
        thumbnail: result.thumbnail,
        category: result.category,
        metadata: result.metadata,
        provider_metadata,
        engines: vec![result.engine_id.clone()],
        positions,
        contributions: vec![EngineContribution {
            engine_id: result.engine_id,
            position: result.position,
            weight: result.engine_weight,
        }],
        score: 0.0,
    })
}

fn choose_stronger(current: &mut String, candidate: &str) {
    let candidate = candidate.trim();
    if candidate.len() > current.trim().len() {
        *current = candidate.to_owned();
    }
}

pub fn deduplicate(results: Vec<NormalizedResult>) -> Vec<NormalizedResult> {
    let mut merged: BTreeMap<String, NormalizedResult> = BTreeMap::new();

    for result in results {
        if let Some(existing) = merged.get_mut(&result.canonical_url) {
            choose_stronger(&mut existing.title, &result.title);
            choose_stronger(&mut existing.content, &result.content);
            if existing.thumbnail.is_none() {
                existing.thumbnail = result.thumbnail;
            }
            if existing.published_at.is_none() {
                existing.published_at = result.published_at;
            }
            for (key, value) in result.metadata {
                existing.metadata.entry(key).or_insert(value);
            }
            for (engine, metadata) in result.provider_metadata {
                existing
                    .provider_metadata
                    .entry(engine)
                    .or_insert(metadata);
            }
            for (engine, position) in result.positions {
                existing
                    .positions
                    .entry(engine)
                    .and_modify(|current| *current = (*current).min(position))
                    .or_insert(position);
            }
            existing.contributions.extend(result.contributions);
            let mut engines: BTreeSet<String> = existing.engines.drain(..).collect();
            engines.extend(result.engines);
            existing.engines = engines.into_iter().collect();
        } else {
            merged.insert(result.canonical_url.clone(), result);
        }
    }

    merged.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map, Value};

    fn provider(engine: &str, title: &str, content: &str, position: u32) -> ProviderResult {
        ProviderResult {
            url: "https://example.com/item?utm_source=test".into(),
            title: title.into(),
            content: content.into(),
            published_at: None,
            thumbnail: None,
            category: "general".into(),
            metadata: Map::from_iter([("provider_field".into(), json!(engine))]),
            engine_id: engine.into(),
            position,
            engine_weight: 1.0,
        }
    }

    #[test]
    fn merges_exact_canonical_matches_and_preserves_provenance() {
        let results = vec![
            normalize_provider_result(provider("a", "Short", "one", 2)).unwrap(),
            normalize_provider_result(provider("b", "A stronger title", "longer content", 1))
                .unwrap(),
        ];
        let merged = deduplicate(results);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "A stronger title");
        assert_eq!(merged[0].engines, vec!["a", "b"]);
        assert_eq!(merged[0].positions["a"], 2);
        assert_eq!(merged[0].positions["b"], 1);
        assert_eq!(merged[0].provider_metadata["a"]["provider_field"], "a");
        assert_eq!(merged[0].provider_metadata["b"]["provider_field"], "b");
    }
}
