use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderCoordinatorCommand, ProviderResult, SearchEngine, SourceKind, StatePolicy,
    DEFAULT_ENGINE_TIMEOUT_MS, DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, DEFAULT_MAX_STEPS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct WolframAlphaEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "wolframalpha",
    display_name: "WolframAlpha",
    categories: &["science"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Beta,
    allowed_hosts: &["api.wolframalpha.com"],
    capabilities: EngineCapabilities {
        paging: false,
        locale: false,
        country: false,
        safe_search: false,
        time_range: false,
    },
    timeout_ms: DEFAULT_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: DEFAULT_MAX_STEPS,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.15,
    parser_version: "wolframalpha-v2-json-v1",
    default_enabled: true,
    allow_http: false,
    state_policy: StatePolicy::DurableCoordinator,
    cache_policy: CachePolicy {
        response_ttl_seconds: 900,
        negative_ttl_seconds: 60,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn build_request(
    query: &NormalizedQuery,
    app_id: Option<&str>,
) -> Result<EngineRequest, EngineFailure> {
    let app_id = app_id.ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineDisabled,
            "WOLFRAM_APP_ID is not configured",
        )
    })?;
    if query.page_number() != 1 || query.time_range.is_some() {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::UnsupportedCapability,
            "WolframAlpha supports only first-page queries without time filters",
        ));
    }
    let mut url = Url::parse("https://api.wolframalpha.com/v2/query").map_err(|error| {
        EngineFailure::new(DESCRIPTOR.id, FailureKind::Internal, error.to_string())
    })?;
    url.query_pairs_mut()
        .append_pair("input", &query.text)
        .append_pair("appid", app_id)
        .append_pair("output", "json")
        .append_pair("format", "plaintext")
        .append_pair("reinterpret", "true");
    Ok(EngineRequest {
        method: EngineMethod::Get,
        url,
        headers: BTreeMap::from([("accept".into(), "application/json".into())]),
        cookies: BTreeMap::new(),
        body: None,
        accepted_content_types: &["application/json"],
    })
}

fn parse_results(body: &[u8], query: &str) -> Result<Vec<ProviderResult>, EngineFailure> {
    let root: Value = serde_json::from_slice(body).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })?;
    let query_result = root.get("queryresult").ok_or_else(|| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            "WolframAlpha response is missing queryresult",
        )
    })?;
    if query_result.get("success").and_then(Value::as_bool) == Some(false) {
        return Ok(Vec::new());
    }
    let pods = query_result
        .get("pods")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "WolframAlpha response is missing pods",
            )
        })?;
    let mut results = Vec::new();
    for pod in pods.iter().take(8) {
        let title = pod
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let plaintext = pod
            .get("subpods")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|subpod| subpod.get("plaintext").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let (Some(title), false) = (title, plaintext.is_empty()) else {
            continue;
        };
        let mut result_url = Url::parse("https://www.wolframalpha.com/input").unwrap();
        result_url
            .query_pairs_mut()
            .append_pair("i", query)
            .append_pair("pod", title);
        results.push(ProviderResult {
            url: result_url.into(),
            title: title.into(),
            content: plaintext,
            published_at: None,
            thumbnail: None,
            category: "science".into(),
            metadata: Map::from_iter([(
                "primary".into(),
                json!(pod.get("primary").and_then(Value::as_bool).unwrap_or(false)),
            )]),
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for WolframAlphaEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let snapshot = context
            .coordinator
            .execute(
                DESCRIPTOR.id,
                ProviderCoordinatorCommand::Snapshot {
                    now_ms: context.now_ms,
                },
            )
            .await?;
        if snapshot
            .cooldown_expires_at_ms
            .is_some_and(|expiry| expiry > context.now_ms)
        {
            return Err(EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineRateLimited,
                "WolframAlpha is in a shared provider cooldown",
            ));
        }
        let app_id = context.secrets.get(DESCRIPTOR.id, "app_id");
        let response = match context
            .http
            .send(
                &DESCRIPTOR,
                build_request(query, app_id.as_deref())?,
                context.deadline,
            )
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
                                now_ms: context.now_ms,
                                failure_kind: failure.kind.as_code().into(),
                                cooldown_ms: 60_000,
                            },
                        )
                        .await;
                }
                return Err(failure);
            }
        };
        let results = parse_results(&response.body, &query.text)?;
        let _ = context
            .coordinator
            .execute(
                DESCRIPTOR.id,
                ProviderCoordinatorCommand::RecordSuccess {
                    now_ms: context.now_ms,
                },
            )
            .await;
        Ok(EngineOutput {
            results,
            next_cursor: None,
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
    fn parses_pods() {
        let results = parse_results(
            br#"{"queryresult":{"success":true,"pods":[{"title":"Result","primary":true,"subpods":[{"plaintext":"42"}]}]}}"#,
            "six times seven",
        )
        .unwrap();
        assert_eq!(results[0].title, "Result");
        assert_eq!(results[0].content, "42");
    }
}
