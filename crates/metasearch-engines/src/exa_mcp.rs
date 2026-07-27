use metasearch_core::{
    BotAuthPolicy, CachePolicy, EngineCapabilities, EngineContext, EngineDescriptor, EngineFailure,
    EngineMaturity, EngineMethod, EngineOutput, EngineRequest, FailureKind, NormalizedQuery,
    ProviderResult, SearchEngine, SourceKind, StatePolicy, DEFAULT_ENGINE_TIMEOUT_MS,
    DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use url::Url;

pub struct ExaMcpEngine;

pub static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
    id: "exa-mcp",
    display_name: "Exa MCP",
    categories: &["general"],
    source_kind: SourceKind::Json,
    maturity: EngineMaturity::Experimental,
    allowed_hosts: &["mcp.exa.ai"],
    capabilities: EngineCapabilities {
        paging: false,
        locale: false,
        country: false,
        safe_search: false,
        time_range: false,
    },
    timeout_ms: DEFAULT_ENGINE_TIMEOUT_MS,
    max_body_bytes: DEFAULT_MAX_BODY_BYTES,
    max_steps: 3,
    max_redirects: DEFAULT_MAX_REDIRECTS,
    weight: 1.0,
    parser_version: "exa-streamable-http-mcp-v1",
    default_enabled: false,
    allow_http: false,
    state_policy: StatePolicy::Stateless,
    cache_policy: CachePolicy {
        response_ttl_seconds: 300,
        negative_ttl_seconds: 60,
    },
    bot_auth_policy: BotAuthPolicy::Disabled,
};

fn rpc_request(api_key: &str, session: Option<&str>, body: Value) -> EngineRequest {
    let mut headers = BTreeMap::from([
        (
            "accept".into(),
            "application/json, text/event-stream".into(),
        ),
        ("content-type".into(), "application/json".into()),
        ("x-api-key".into(), api_key.into()),
    ]);
    if let Some(session) = session {
        headers.insert("mcp-session-id".into(), session.into());
    }
    EngineRequest {
        method: EngineMethod::Post,
        url: Url::parse("https://mcp.exa.ai/mcp").unwrap(),
        headers,
        cookies: BTreeMap::new(),
        body: Some(serde_json::to_vec(&body).unwrap()),
        accepted_content_types: &["application/json", "text/event-stream"],
    }
}

fn parse_rpc(body: &[u8]) -> Result<Value, EngineFailure> {
    if let Ok(value) = serde_json::from_slice(body) {
        return Ok(value);
    }
    let text = String::from_utf8_lossy(body);
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Exa MCP response contained no JSON or SSE data event",
            )
        })?;
    serde_json::from_str(data).map_err(|error| {
        EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        )
    })
}

fn parse_results(root: &Value) -> Result<Vec<ProviderResult>, EngineFailure> {
    if let Some(error) = root.get("error") {
        return Err(EngineFailure::new(
            DESCRIPTOR.id,
            FailureKind::EngineParseFailed,
            error.to_string(),
        ));
    }
    let content = root
        .pointer("/result/content")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Exa MCP tool result is missing content",
            )
        })?;
    let embedded = content
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .find_map(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| json!({"results":[]}));
    let items = embedded
        .get("results")
        .and_then(Value::as_array)
        .or_else(|| embedded.as_array())
        .ok_or_else(|| {
            EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                "Exa MCP tool result has no result array",
            )
        })?;
    let mut results = Vec::new();
    for item in items {
        let Some(url) = item
            .get("url")
            .and_then(Value::as_str)
            .filter(|value| Url::parse(value).is_ok())
        else {
            continue;
        };
        let title = item.get("title").and_then(Value::as_str).unwrap_or(url);
        results.push(ProviderResult {
            url: url.into(),
            title: title.into(),
            content: item
                .get("text")
                .or_else(|| item.get("summary"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            published_at: item
                .get("publishedDate")
                .and_then(Value::as_str)
                .map(str::to_owned),
            thumbnail: item.get("image").and_then(Value::as_str).map(str::to_owned),
            category: "general".into(),
            metadata: Map::new(),
            engine_id: DESCRIPTOR.id.into(),
            position: (results.len() + 1) as u32,
            engine_weight: DESCRIPTOR.weight,
        });
    }
    Ok(results)
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for ExaMcpEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        &DESCRIPTOR
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        let api_key = context
            .secrets
            .get(DESCRIPTOR.id, "api_key")
            .ok_or_else(|| {
                EngineFailure::new(
                    DESCRIPTOR.id,
                    FailureKind::EngineDisabled,
                    "EXA_API_KEY is not configured",
                )
            })?;
        let initialize = context
            .http
            .send(
                &DESCRIPTOR,
                rpc_request(
                    &api_key,
                    None,
                    json!({
                        "jsonrpc":"2.0",
                        "id":1,
                        "method":"initialize",
                        "params":{
                            "protocolVersion":"2025-06-18",
                            "capabilities":{},
                            "clientInfo":{"name":"searxflare","version":"0.1.0"}
                        }
                    }),
                ),
                context.deadline,
            )
            .await?;
        let initialized = parse_rpc(&initialize.body)?;
        if initialized.get("error").is_some() {
            return Err(EngineFailure::new(
                DESCRIPTOR.id,
                FailureKind::EngineParseFailed,
                initialized["error"].to_string(),
            ));
        }
        let session = initialize.headers.get("mcp-session-id").map(String::as_str);
        let initialized_notification = context
            .http
            .send(
                &DESCRIPTOR,
                rpc_request(
                    &api_key,
                    session,
                    json!({
                        "jsonrpc":"2.0",
                        "method":"notifications/initialized"
                    }),
                ),
                context.deadline,
            )
            .await?;
        let response = context
            .http
            .send(
                &DESCRIPTOR,
                rpc_request(
                    &api_key,
                    session,
                    json!({
                        "jsonrpc":"2.0",
                        "id":2,
                        "method":"tools/call",
                        "params":{
                            "name":"web_search_exa",
                            "arguments":{"query":query.text,"numResults":query.limit}
                        }
                    }),
                ),
                context.deadline,
            )
            .await?;
        let root = parse_rpc(&response.body)?;
        let results = parse_results(&root)?;
        Ok(EngineOutput {
            results,
            next_cursor: None,
            upstream_requests: 3,
            response_bytes: initialize.body.len()
                + initialized_notification.body.len()
                + response.body.len(),
            parse_ms: 0,
            redirect_count: initialize
                .redirect_count
                .saturating_add(initialized_notification.redirect_count)
                .saturating_add(response.redirect_count),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_and_sse_tool_results() {
        let result = json!({
            "jsonrpc":"2.0",
            "id":2,
            "result":{"content":[{"type":"text","text":"{\"results\":[{\"title\":\"Example\",\"url\":\"https://example.com\",\"summary\":\"Result\"}]}"}]}
        });
        let parsed = parse_results(&result).unwrap();
        assert_eq!(parsed[0].title, "Example");

        let sse = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        assert_eq!(parse_rpc(sse).unwrap()["id"], 1);
    }
}
