use metasearch_core::{
    deduplicate, normalize_provider_result, rank_results, ProviderResult, RankingStrategy,
};
use serde_json::Map;

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

fn main() {
    let _profiler = dhat::Profiler::new_heap();
    let results = (0..250)
        .map(|index| ProviderResult {
            url: format!(
                "https://example.com/result/{}?utm_source=profile",
                index % 200
            ),
            title: format!("Cloudflare Rust metasearch result {index}"),
            content: "Bounded provider result used for deterministic heap profiling.".repeat(4),
            published_at: Some("2026-07-27T00:00:00Z".into()),
            thumbnail: None,
            category: "general".into(),
            metadata: Map::new(),
            engine_id: format!("engine-{}", index % 5),
            position: (index % 20 + 1) as u32,
            engine_weight: 1.0,
        })
        .map(normalize_provider_result)
        .collect::<Result<Vec<_>, _>>()
        .expect("profile fixtures must normalize");
    let ranked = rank_results(
        deduplicate(results),
        RankingStrategy::QueryAwareV1,
        "cloudflare rust metasearch",
    )
    .expect("profile fixtures must rank");
    assert!(ranked.len() <= 200);
    let stats = dhat::HeapStats::get();
    assert!(
        stats.max_bytes <= 64 * 1024 * 1024,
        "peak live heap {} bytes exceeds the 64 MiB request budget",
        stats.max_bytes
    );
}
