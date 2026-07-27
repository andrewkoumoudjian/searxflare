use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderCoordinatorCommand, ProviderResult, SafeSearch, SearchEngine, SourceKind, StatePolicy,
    TimeRange, DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
    HTML_ENGINE_TIMEOUT_MS,
};
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use serde_json::Map;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use url::Url;

pub struct DuckDuckGoHtmlEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "duckduckgo-html",
    display_name: "DuckDuckGo HTML",
    categories: &["general"],
    source_kind: SourceKind::Html,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &["html.duckduckgo.com", "duckduckgo.com"],
    capabilities: EngineCapabilities {
        paging: true,
        locale: true,
        country: false,
        safe_search: true,
        time_range: true,
    },
    timeout_ms: HTML_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.0,
    parser_version: "duckduckgo-html-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::DurableCoordinator,
    cache_policy: CachePolicy {
        response_ttl_seconds: 180,
        negative_ttl_seconds: 30,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

const SELECTORS: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results, .result--no-result"),
    item: "#links .web-result",
    title: "h2 a",
    url: "h2 a",
    description: Some("a.result__snippet"),
    thumbnail: None,
};

fn locale_code(locale: Option<&str>) -> String {
    locale
        .map(|locale| locale.replace('_', "-").to_ascii_lowercase())
        .filter(|locale| locale.len() >= 2)
        .unwrap_or_else(|| "wt-wt".into())
}

fn safe_search_code(safe_search: SafeSearch) -> &'static str {
    match safe_search {
        SafeSearch::Off => "-2",
        SafeSearch::Moderate => "-1",
        SafeSearch::Strict => "1",
    }
}

fn time_range_code(time_range: Option<TimeRange>) -> Option<&'static str> {
    match time_range {
        Some(TimeRange::Day) => Some("d"),
        Some(TimeRange::Week) => Some("w"),
        Some(TimeRange::Month) => Some("m"),
        Some(TimeRange::Year) => Some("y"),
        None => None,
    }
}

fn unwrap_redirect(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return url.to_owned();
    };
    let host = parsed.host_str().unwrap_or_default();
    if !matches!(host, "duckduckgo.com" | "html.duckduckgo.com") {
        return url.to_owned();
    }
    if parsed.path() != "/l/" && parsed.path() != "/l" {
        return url.to_owned();
    }
    parsed
        .query_pairs()
        .find(|(name, _)| name == "uddg")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_else(|| url.to_owned())
}

fn query_state_key(query: &NormalizedQuery) -> String {
    let digest = Sha256::digest(
        format!(
            "{}\0{:?}\0{:?}\0{:?}",
            query.text, query.locale, query.safe_search, query.time_range
        )
        .as_bytes(),
    );
    format!("continuation:{digest:x}")
}

fn extract_vqd(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    for marker in ["name=\"vqd\" value=\"", "vqd='", "vqd=\""] {
        let Some(start) = text.find(marker) else {
            continue;
        };
        let value = &text[start + marker.len()..];
        let delimiter = if marker.ends_with('\'') { '\'' } else { '"' };
        if let Some(end) = value.find(delimiter) {
            let value = value[..end].trim();
            if !value.is_empty()
                && value.len() <= 256
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            {
                return Some(value.into());
            }
        }
    }
    None
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for DuckDuckGoHtmlEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        if query.page_number() > 20 {
            return Err(EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::UnsupportedCapability,
                "DuckDuckGo HTML paging is capped at twenty pages",
            ));
        }
        let now_ms = context.now_ms;
        let cooldown = context
            .coordinator
            .execute(
                DESCRIPTOR.id,
                ProviderCoordinatorCommand::Snapshot { now_ms },
            )
            .await?;
        if cooldown
            .cooldown_expires_at_ms
            .is_some_and(|expiry| expiry > now_ms)
        {
            return Err(EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineRateLimited,
                "DuckDuckGo is in a shared provider cooldown",
            ));
        }
        let state_key = query_state_key(query);
        let coordination_key = format!("{}:{state_key}", DESCRIPTOR.id);
        let mut continuation = query.cursor.clone();
        if continuation.is_none() && query.page_number() > 1 {
            continuation = context
                .state
                .get(DESCRIPTOR.id, &state_key)
                .await?
                .and_then(|bytes| String::from_utf8(bytes).ok());
        }
        if query.page_number() > 1 && continuation.is_none() {
            return Err(EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::InvalidRequest,
                "DuckDuckGo continuation cursor is missing or expired",
            ));
        }
        if query.page_number() == 1 {
            let lease = context
                .coordinator
                .execute(
                    &coordination_key,
                    ProviderCoordinatorCommand::AcquireRefreshLease {
                        now_ms,
                        lease_ms: 5_000,
                    },
                )
                .await?;
            if !lease.refresh_lease_acquired {
                return Err(EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineRateLimited,
                    "DuckDuckGo continuation initialization is already in progress",
                ));
            }
        }
        let url = Url::parse("https://html.duckduckgo.com/html/").map_err(|error| {
            EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
        })?;
        let locale = locale_code(query.locale.as_deref());
        let safe_search = safe_search_code(query.safe_search);
        let mut form = vec![
            ("q", query.text.as_str()),
            ("b", ""),
            ("kl", locale.as_str()),
            ("kp", safe_search),
        ];
        let offset = query
            .page_number()
            .saturating_sub(1)
            .saturating_mul(u32::from(query.limit));
        let offset_string = offset.to_string();
        let result_start = offset.saturating_add(1).to_string();
        if query.page_number() > 1 {
            form.push(("s", offset_string.as_str()));
            form.push(("dc", result_start.as_str()));
        }
        if let Some(vqd) = continuation.as_deref() {
            form.push(("vqd", vqd));
        }
        if let Some(code) = time_range_code(query.time_range) {
            form.push(("df", code));
        }
        let body = serde_urlencoded::to_string(form)
            .map_err(|error| {
                EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
            })?
            .into_bytes();
        let mut cookies = BTreeMap::from([
            ("kl".into(), locale.clone()),
            ("kp".into(), safe_search.into()),
        ]);
        if let Some(code) = time_range_code(query.time_range) {
            cookies.insert("df".into(), code.into());
        }
        let request = EngineRequest {
            method: EngineMethod::Post,
            url: url.clone(),
            headers: BTreeMap::from([
                ("accept".into(), "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".into()),
                ("accept-language".into(), query.locale.clone().unwrap_or_else(|| "en-US,en;q=0.8".into())),
                ("content-type".into(), "application/x-www-form-urlencoded".into()),
                ("referer".into(), "https://html.duckduckgo.com/".into()),
                ("sec-fetch-dest".into(), "document".into()),
                ("sec-fetch-mode".into(), "navigate".into()),
                ("sec-fetch-site".into(), "same-origin".into()),
                ("sec-fetch-user".into(), "?1".into()),
                ("user-agent".into(), "Mozilla/5.0 (compatible; Searxflare/0.1; +https://github.com/andrewkoumoudjian/searxflare)".into()),
            ]),
            cookies,
            body: Some(body),
            accepted_content_types: &["text/html", "application/xhtml+xml"],
        };
        let response = match context
            .http
            .send(&DESCRIPTOR, request, context.deadline)
            .await
        {
            Ok(response) => response,
            Err(failure) => {
                if matches!(
                    failure.kind,
                    FailureKind::EngineRateLimited | FailureKind::EngineChallenged
                ) {
                    let _ = context
                        .coordinator
                        .execute(
                            DESCRIPTOR.id,
                            ProviderCoordinatorCommand::RecordFailure {
                                now_ms,
                                failure_kind: failure.kind.as_code().into(),
                                cooldown_ms: 60_000,
                            },
                        )
                        .await;
                }
                return Err(failure);
            }
        };
        let parsed =
            parse_selector_results(&response.body, &url, &SELECTORS, 20).map_err(|error| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineParseFailed,
                    error.to_string(),
                )
            })?;
        let results = parsed
            .into_iter()
            .enumerate()
            .map(|(index, result)| ProviderResult {
                url: unwrap_redirect(&result.url),
                title: result.title,
                content: result.description.unwrap_or_default(),
                published_at: None,
                thumbnail: None,
                category: "general".into(),
                metadata: Map::new(),
                engine_id: DESCRIPTOR.id.into(),
                position: (index + 1) as u32,
                engine_weight: DESCRIPTOR.weight,
            })
            .collect();
        let next_cursor = extract_vqd(&response.body).or(continuation);
        if let Some(cursor) = next_cursor.as_deref() {
            context
                .state
                .put(DESCRIPTOR.id, &state_key, cursor.as_bytes(), Some(900))
                .await?;
        }
        let _ = context
            .coordinator
            .execute(
                &coordination_key,
                ProviderCoordinatorCommand::RecordSuccess { now_ms },
            )
            .await;
        let _ = context
            .coordinator
            .execute(
                DESCRIPTOR.id,
                ProviderCoordinatorCommand::RecordSuccess { now_ms },
            )
            .await;
        Ok(EngineOutput {
            results,
            next_cursor,
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

    #[test]
    fn unwraps_only_duckduckgo_redirects() {
        assert_eq!(
            unwrap_redirect("https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fitem"),
            "https://example.com/item"
        );
        assert_eq!(
            unwrap_redirect("https://example.com/l/?uddg=https://evil.test"),
            "https://example.com/l/?uddg=https://evil.test"
        );
    }
}
