use futures_util::{stream::FuturesUnordered, StreamExt};
use metasearch_api::{
    ApiError, EngineCatalogueResponse, EngineDescriptorResponse, ErrorCode, SearchRequest,
    SearchResponse, SearxCompatResponse, ValidatedSearchRequest,
};
use metasearch_core::{
    build_cache_key, deduplicate, normalize_provider_result, rank_results, CacheKeyInput,
    CacheStatus, Deadline, EngineContext, EngineExecutionReport, EngineFailure, EngineOutput,
    FailureKind, NormalizedQuery, RankingStrategy, SafeSearch, SearchEngine, TimeRange,
    ENGINE_REGISTRY_VERSION,
};
use metasearch_engines::{default_engine_ids, find_engine, registry, RegisteredEngine};
use metasearch_http::WorkerFetchClient;
use metasearch_state::NoopEngineState;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, rc::Rc};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use worker::{
    console_log, AnalyticsEngineDataPointBuilder, Cache, Context, Date, Env, Headers, Request,
    Response, ResponseBuilder, RouteContext, Router,
};

struct AppData {
    execution: Rc<Context>,
}

struct EngineRun {
    output: Result<EngineOutput, EngineFailure>,
    report: EngineExecutionReport,
}

pub async fn handle(req: Request, env: Env, ctx: Context) -> worker::Result<Response> {
    Router::with_data(AppData {
        execution: Rc::new(ctx),
    })
    .get_async("/healthz", healthz)
    .get_async("/readyz", readyz)
    .get_async("/v1/search", search_get)
    .post_async("/v1/search", search_post)
    .get_async("/v1/engines", engines)
    .get_async("/v1/engines/:engine_id", engine)
    .get_async("/v1/engines/:engine_id/search", engine_search)
    .get_async("/search", searx_compat)
    .or_else_any_method_async("/*path", not_found)
    .run(req, env)
    .await
}

fn request_id() -> String {
    Uuid::new_v4().to_string()
}

fn response_headers(request_id: &str, content_type: &str) -> worker::Result<Headers> {
    let headers = Headers::new();
    headers.set("content-type", content_type)?;
    headers.set("x-request-id", request_id)?;
    headers.set("cache-control", "private, no-store")?;
    Ok(headers)
}

fn json_response<T: Serialize>(
    value: &T,
    status: u16,
    request_id: &str,
) -> worker::Result<Response> {
    let bytes = serde_json::to_vec(value)?;
    Ok(ResponseBuilder::new()
        .with_status(status)
        .with_headers(response_headers(
            request_id,
            "application/json; charset=utf-8",
        )?)
        .fixed(bytes))
}

fn problem_response(error: ApiError, instance: &str, request_id: &str) -> worker::Result<Response> {
    let problem = error.problem(instance, request_id);
    let bytes = serde_json::to_vec(&problem)?;
    Ok(ResponseBuilder::new()
        .with_status(problem.status)
        .with_headers(response_headers(
            request_id,
            "application/problem+json; charset=utf-8",
        )?)
        .fixed(bytes))
}

fn sha256_hex(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn authenticate(req: &Request, env: &Env) -> Result<(), ApiError> {
    let authorization = req
        .headers()
        .get("authorization")
        .map_err(|_| {
            ApiError::new(
                ErrorCode::AuthenticationRequired,
                "invalid Authorization header",
            )
        })?
        .ok_or_else(|| {
            ApiError::new(
                ErrorCode::AuthenticationRequired,
                "Bearer API key is required",
            )
        })?;
    let key = authorization.strip_prefix("Bearer ").ok_or_else(|| {
        ApiError::new(
            ErrorCode::AuthenticationRequired,
            "Authorization must use the Bearer scheme",
        )
    })?;
    let expected = env
        .secret("API_KEY_SHA256")
        .map_err(|_| ApiError::new(ErrorCode::InternalError, "API key secret is not configured"))?
        .to_string()
        .trim()
        .to_ascii_lowercase();
    let actual = sha256_hex(key.as_bytes());
    if expected.len() != actual.len()
        || expected.as_bytes().ct_eq(actual.as_bytes()).unwrap_u8() != 1
    {
        return Err(ApiError::new(
            ErrorCode::AuthenticationRequired,
            "API key is invalid",
        ));
    }
    Ok(())
}

async fn healthz(_req: Request, _ctx: RouteContext<AppData>) -> worker::Result<Response> {
    json_response(&serde_json::json!({"status":"ok"}), 200, &request_id())
}

async fn readyz(_req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let id = request_id();
    if ctx.env.secret("API_KEY_SHA256").is_ok() {
        json_response(
            &serde_json::json!({"status":"ready","registry_version":ENGINE_REGISTRY_VERSION}),
            200,
            &id,
        )
    } else {
        json_response(
            &serde_json::json!({"status":"not_ready","reason":"API_KEY_SHA256 is missing"}),
            503,
            &id,
        )
    }
}

fn parse_csv(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn parse_safe_search(value: Option<String>) -> Option<SafeSearch> {
    match value.as_deref() {
        Some("0" | "off") => Some(SafeSearch::Off),
        Some("1" | "moderate") => Some(SafeSearch::Moderate),
        Some("2" | "strict") => Some(SafeSearch::Strict),
        _ => None,
    }
}

fn parse_time_range(value: Option<String>) -> Option<TimeRange> {
    match value.as_deref() {
        Some("day") => Some(TimeRange::Day),
        Some("week") => Some(TimeRange::Week),
        Some("month") => Some(TimeRange::Month),
        Some("year") => Some(TimeRange::Year),
        _ => None,
    }
}

fn parse_ranking(value: Option<String>) -> Option<RankingStrategy> {
    match value.as_deref() {
        Some("rrf-v1") => Some(RankingStrategy::RrfV1),
        Some("searx-compat-v1") => Some(RankingStrategy::SearxCompatV1),
        _ => None,
    }
}

fn request_from_url(req: &Request, compatibility: bool) -> Result<SearchRequest, ApiError> {
    let url = req
        .url()
        .map_err(|error| ApiError::new(ErrorCode::InvalidRequest, error.to_string()))?;
    let values: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    if compatibility && values.get("format").map(String::as_str) != Some("json") {
        return Err(ApiError::new(
            ErrorCode::InvalidRequest,
            "the compatibility route only supports format=json",
        ));
    }
    Ok(SearchRequest {
        query: (!compatibility)
            .then(|| values.get("query").cloned())
            .flatten(),
        q: values.get("q").cloned(),
        engines: parse_csv(values.get("engines").cloned()),
        categories: parse_csv(values.get("categories").cloned()),
        page: values
            .get(if compatibility { "pageno" } else { "page" })
            .and_then(|value| value.parse().ok()),
        cursor: values.get("cursor").cloned(),
        limit: values.get("limit").and_then(|value| value.parse().ok()),
        locale: values
            .get(if compatibility { "language" } else { "locale" })
            .cloned(),
        country: values.get("country").cloned(),
        safe_search: parse_safe_search(
            values
                .get(if compatibility {
                    "safesearch"
                } else {
                    "safe_search"
                })
                .cloned(),
        ),
        time_range: parse_time_range(values.get("time_range").cloned()),
        ranking: parse_ranking(values.get("ranking").cloned()),
        timeout_ms: values
            .get("timeout_ms")
            .and_then(|value| value.parse().ok()),
    })
}

fn select_engines(query: &NormalizedQuery) -> Result<Vec<&'static RegisteredEngine>, ApiError> {
    let requested = if query.engines.is_empty() {
        let category_matches: Vec<String> = if query.categories.is_empty() {
            default_engine_ids()
        } else {
            registry()
                .iter()
                .filter(|engine| {
                    engine.descriptor().categories.iter().any(|category| {
                        query
                            .categories
                            .iter()
                            .any(|requested| requested == category)
                    })
                })
                .map(|engine| engine.descriptor().id.to_owned())
                .collect()
        };
        category_matches
    } else {
        query.engines.clone()
    };

    let mut selected = Vec::new();
    for engine_id in requested {
        let engine = find_engine(&engine_id).ok_or_else(|| {
            ApiError::new(
                ErrorCode::UnknownEngine,
                format!("unknown engine: {engine_id}"),
            )
        })?;
        if !engine.descriptor().default_enabled && query.engines.is_empty() {
            continue;
        }
        selected.push(engine);
    }
    if selected.is_empty() {
        return Err(ApiError::new(
            ErrorCode::NoEngineSucceeded,
            "no enabled engine matches the request",
        ));
    }
    Ok(selected)
}

fn engine_cache_key(query: &NormalizedQuery, engine: &'static RegisteredEngine) -> String {
    let engines = vec![engine.descriptor().id.to_owned()];
    let parsers = vec![format!(
        "{}:{}",
        engine.descriptor().id,
        engine.descriptor().parser_version
    )];
    build_cache_key(&CacheKeyInput {
        normalized_query: &query.text,
        engine_ids: &engines,
        categories: &query.categories,
        page: query.page,
        cursor_hash: query
            .cursor
            .as_deref()
            .map(|cursor| sha256_hex(cursor.as_bytes()))
            .as_deref(),
        limit: query.limit,
        locale: query.locale.as_deref(),
        country: query.country.as_deref(),
        safe_search: query.safe_search,
        time_range: query.time_range,
        parser_versions: &parsers,
        ranking: query.ranking,
    })
    .replace("/search/", &format!("/engine/{}/", engine.descriptor().id))
}

async fn run_engine(
    engine: &'static RegisteredEngine,
    query: &NormalizedQuery,
    http: &WorkerFetchClient,
    state: &NoopEngineState,
    deadline: Deadline,
    request_id: &str,
    execution: Rc<Context>,
) -> EngineRun {
    let started = Date::now().as_millis() as u64;
    let cache_key = engine_cache_key(query, engine);
    let cache = Cache::default();
    if let Ok(Some(mut cached)) = cache.get(&cache_key, true).await {
        let negative = cached
            .headers()
            .get("x-searxflare-negative")
            .ok()
            .flatten()
            .as_deref()
            == Some("1");
        if negative {
            if let Ok(failure) = cached.json::<EngineFailure>().await {
                return EngineRun {
                    output: Err(failure.clone()),
                    report: EngineExecutionReport {
                        engine_id: engine.descriptor().id.into(),
                        duration_ms: (Date::now().as_millis() as u64).saturating_sub(started),
                        cache_status: CacheStatus::NegativeHit,
                        result_count: 0,
                        failure_kind: Some(failure.kind.as_code().into()),
                        parser_version: Some(engine.descriptor().parser_version.into()),
                    },
                };
            }
        } else if let Ok(output) = cached.json::<EngineOutput>().await {
            return EngineRun {
                report: EngineExecutionReport {
                    engine_id: engine.descriptor().id.into(),
                    duration_ms: (Date::now().as_millis() as u64).saturating_sub(started),
                    cache_status: CacheStatus::Hit,
                    result_count: output.results.len(),
                    failure_kind: None,
                    parser_version: Some(engine.descriptor().parser_version.into()),
                },
                output: Ok(output),
            };
        }
    }

    let result = engine
        .search(
            query,
            &EngineContext {
                http,
                state,
                deadline,
                request_id,
            },
        )
        .await;
    let duration_ms = (Date::now().as_millis() as u64).saturating_sub(started);
    match result {
        Ok(output) => {
            if let Ok(cache_response) = ResponseBuilder::new()
                .with_header(
                    "cache-control",
                    &format!(
                        "s-maxage={}",
                        engine.descriptor().cache_policy.response_ttl_seconds
                    ),
                )
                .and_then(|builder| builder.from_json(&output))
            {
                execution.wait_until(async move {
                    let _ = Cache::default().put(cache_key, cache_response).await;
                });
            }
            EngineRun {
                report: EngineExecutionReport {
                    engine_id: engine.descriptor().id.into(),
                    duration_ms,
                    cache_status: CacheStatus::Miss,
                    result_count: output.results.len(),
                    failure_kind: None,
                    parser_version: Some(engine.descriptor().parser_version.into()),
                },
                output: Ok(output),
            }
        }
        Err(failure) => {
            if let Ok(cache_response) = ResponseBuilder::new()
                .with_header(
                    "cache-control",
                    &format!(
                        "s-maxage={}",
                        engine.descriptor().cache_policy.negative_ttl_seconds
                    ),
                )
                .and_then(|builder| builder.with_header("x-searxflare-negative", "1"))
                .and_then(|builder| builder.from_json(&failure))
            {
                execution.wait_until(async move {
                    let _ = Cache::default().put(cache_key, cache_response).await;
                });
            }
            EngineRun {
                report: EngineExecutionReport {
                    engine_id: engine.descriptor().id.into(),
                    duration_ms,
                    cache_status: CacheStatus::Miss,
                    result_count: 0,
                    failure_kind: Some(failure.kind.as_code().into()),
                    parser_version: Some(engine.descriptor().parser_version.into()),
                },
                output: Err(failure),
            }
        }
    }
}

fn aggregate_cache_key(query: &NormalizedQuery, selected: &[&'static RegisteredEngine]) -> String {
    let engines: Vec<String> = selected
        .iter()
        .map(|engine| engine.descriptor().id.into())
        .collect();
    let parsers: Vec<String> = selected
        .iter()
        .map(|engine| {
            format!(
                "{}:{}",
                engine.descriptor().id,
                engine.descriptor().parser_version
            )
        })
        .collect();
    let cursor_hash = query
        .cursor
        .as_deref()
        .map(|cursor| sha256_hex(cursor.as_bytes()));
    build_cache_key(&CacheKeyInput {
        normalized_query: &query.text,
        engine_ids: &engines,
        categories: &query.categories,
        page: query.page,
        cursor_hash: cursor_hash.as_deref(),
        limit: query.limit,
        locale: query.locale.as_deref(),
        country: query.country.as_deref(),
        safe_search: query.safe_search,
        time_range: query.time_range,
        parser_versions: &parsers,
        ranking: query.ranking,
    })
}

fn failure_code(failure: &EngineFailure) -> ErrorCode {
    match failure.kind {
        FailureKind::EngineDisabled => ErrorCode::EngineDisabled,
        FailureKind::UnsupportedCapability => ErrorCode::UnsupportedCapability,
        FailureKind::EngineTimeout => ErrorCode::EngineTimeout,
        FailureKind::EngineRateLimited => ErrorCode::EngineRateLimited,
        FailureKind::EngineChallenged => ErrorCode::EngineChallenged,
        FailureKind::EngineAccessDenied => ErrorCode::EngineAccessDenied,
        FailureKind::EngineResponseTooLarge => ErrorCode::EngineResponseTooLarge,
        FailureKind::EngineInvalidContentType => ErrorCode::EngineInvalidContentType,
        FailureKind::EngineParseFailed => ErrorCode::EngineParseFailed,
        FailureKind::InvalidRequest => ErrorCode::InvalidRequest,
        FailureKind::Internal => ErrorCode::InternalError,
    }
}

fn log_search(query: &NormalizedQuery, response: &SearchResponse, duration_ms: u64) {
    let successful: Vec<&str> = response
        .engines
        .iter()
        .filter(|report| report.failure_kind.is_none())
        .map(|report| report.engine_id.as_str())
        .collect();
    let failed: Vec<&str> = response
        .engines
        .iter()
        .filter(|report| report.failure_kind.is_some())
        .map(|report| report.engine_id.as_str())
        .collect();
    console_log!(
        "{}",
        serde_json::json!({
            "event":"search_complete",
            "request_id":response.request_id,
            "query_hash":sha256_hex(query.text.as_bytes()),
            "selected_engines":response.engines.iter().map(|report| report.engine_id.as_str()).collect::<Vec<_>>(),
            "successful_engines":successful,
            "failed_engines":failed,
            "cache_status":if response.cached {"hit"} else {"miss"},
            "duration_ms":duration_ms,
            "result_count":response.result_count,
            "partial":response.partial,
        })
    );
}

fn write_analytics(env: &Env, reports: &[EngineExecutionReport]) {
    let Ok(dataset) = env.analytics_engine("SEARCH_ANALYTICS") else {
        return;
    };
    for report in reports {
        let outcome = if report.failure_kind.is_some() {
            "failure"
        } else {
            "success"
        };
        let cache_status = format!("{:?}", report.cache_status).to_ascii_lowercase();
        let _ = AnalyticsEngineDataPointBuilder::new()
            .indexes([report.engine_id.as_str()])
            .add_blob(report.engine_id.as_str())
            .add_blob("search")
            .add_blob(outcome)
            .add_blob(report.failure_kind.as_deref().unwrap_or("none"))
            .add_blob(report.parser_version.as_deref().unwrap_or("unknown"))
            .add_blob(cache_status)
            .add_blob("unknown")
            .add_double(report.duration_ms as f64)
            .add_double(0.0)
            .add_double(0.0)
            .add_double(report.result_count as f64)
            .add_double(0.0)
            .write_to(&dataset);
    }
}

async fn execute_search(
    query: NormalizedQuery,
    ctx: &RouteContext<AppData>,
    id: &str,
) -> Result<SearchResponse, ApiError> {
    let selected = select_engines(&query)?;
    let cache_key = aggregate_cache_key(&query, &selected);
    if let Ok(Some(mut cached)) = Cache::default().get(&cache_key, true).await {
        if let Ok(mut response) = cached.json::<SearchResponse>().await {
            response.request_id = id.into();
            response.cached = true;
            return Ok(response);
        }
    }

    let http = WorkerFetchClient;
    let state = NoopEngineState;
    let deadline = Deadline::from_now(Date::now().as_millis() as u64, query.timeout_ms);
    let mut futures = FuturesUnordered::new();
    for engine in selected.iter().copied() {
        futures.push(run_engine(
            engine,
            &query,
            &http,
            &state,
            deadline,
            id,
            Rc::clone(&ctx.data.execution),
        ));
    }

    let mut reports = Vec::new();
    let mut normalized = Vec::new();
    let mut failures = Vec::new();
    let mut successful_engines = 0usize;
    while let Some(run) = futures.next().await {
        let EngineRun { output, mut report } = run;
        match output {
            Ok(output) => {
                let mut engine_invalid = None;
                let mut engine_results = Vec::new();
                for result in output.results {
                    match normalize_provider_result(result) {
                        Ok(result) => engine_results.push(result),
                        Err(error) => {
                            engine_invalid = Some(error.to_string());
                            break;
                        }
                    }
                }
                if let Some(message) = engine_invalid {
                    report.result_count = 0;
                    report.failure_kind = Some(FailureKind::EngineParseFailed.as_code().into());
                    failures.push(EngineFailure::new(
                        report.engine_id.clone(),
                        FailureKind::EngineParseFailed,
                        message,
                    ));
                } else {
                    successful_engines += 1;
                    normalized.extend(engine_results);
                }
            }
            Err(failure) => failures.push(failure),
        }
        reports.push(report);
    }
    reports.sort_by(|left, right| left.engine_id.cmp(&right.engine_id));

    if successful_engines == 0 {
        let first = failures.first().ok_or_else(|| {
            ApiError::new(
                ErrorCode::InternalError,
                "engine execution completed without a success or failure",
            )
        })?;
        return Err(ApiError::new(
            ErrorCode::NoEngineSucceeded,
            format!(
                "all selected engines failed; first failure was {}: {}",
                failure_code(first).as_str(),
                first.message
            ),
        ));
    }

    let mut results = rank_results(deduplicate(normalized), query.ranking)
        .map_err(|error| ApiError::new(ErrorCode::InternalError, error.to_string()))?;
    results.truncate(usize::from(query.limit));
    let partial = !failures.is_empty();
    let response = SearchResponse {
        request_id: id.into(),
        query: query.text.clone(),
        ranking: query.ranking,
        partial,
        cached: false,
        result_count: results.len(),
        results,
        engines: reports,
    };

    let ttl = if partial {
        20
    } else {
        selected
            .iter()
            .map(|engine| engine.descriptor().cache_policy.response_ttl_seconds)
            .min()
            .unwrap_or(180)
    };
    if let Ok(cache_response) = ResponseBuilder::new()
        .with_header("cache-control", &format!("s-maxage={ttl}"))
        .and_then(|builder| builder.from_json(&response))
    {
        let execution = Rc::clone(&ctx.data.execution);
        execution.wait_until(async move {
            let _ = Cache::default().put(cache_key, cache_response).await;
        });
    }
    write_analytics(&ctx.env, &response.engines);
    Ok(response)
}

async fn handle_search_request(
    req: Request,
    ctx: RouteContext<AppData>,
    search_request: SearchRequest,
    require_auth: bool,
) -> worker::Result<Response> {
    let id = request_id();
    let path = req.path();
    if require_auth {
        if let Err(error) = authenticate(&req, &ctx.env) {
            return problem_response(error, &path, &id);
        }
    }
    let query = match ValidatedSearchRequest::try_from(search_request) {
        Ok(validated) => validated.0,
        Err(error) => return problem_response(error, &path, &id),
    };
    let started = Date::now().as_millis() as u64;
    match execute_search(query.clone(), &ctx, &id).await {
        Ok(response) => {
            log_search(
                &query,
                &response,
                (Date::now().as_millis() as u64).saturating_sub(started),
            );
            json_response(&response, 200, &id)
        }
        Err(error) => problem_response(error, &path, &id),
    }
}

async fn search_get(req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let request = match request_from_url(&req, false) {
        Ok(request) => request,
        Err(error) => return problem_response(error, &req.path(), &request_id()),
    };
    handle_search_request(req, ctx, request, true).await
}

async fn search_post(mut req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let id = request_id();
    if let Err(error) = authenticate(&req, &ctx.env) {
        return problem_response(error, &req.path(), &id);
    }
    let request = match req.json::<SearchRequest>().await {
        Ok(request) => request,
        Err(error) => {
            return problem_response(
                ApiError::new(ErrorCode::InvalidRequest, error.to_string()),
                &req.path(),
                &id,
            )
        }
    };
    handle_search_request(req, ctx, request, false).await
}

async fn engines(req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let id = request_id();
    if let Err(error) = authenticate(&req, &ctx.env) {
        return problem_response(error, &req.path(), &id);
    }
    let response = EngineCatalogueResponse {
        registry_version: ENGINE_REGISTRY_VERSION,
        engines: registry()
            .iter()
            .map(|engine| EngineDescriptorResponse {
                descriptor: engine.descriptor(),
            })
            .collect(),
    };
    json_response(&response, 200, &id)
}

async fn engine(req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let id = request_id();
    if let Err(error) = authenticate(&req, &ctx.env) {
        return problem_response(error, &req.path(), &id);
    }
    let engine_id = ctx
        .param("engine_id")
        .map(String::as_str)
        .unwrap_or_default();
    match find_engine(engine_id) {
        Some(engine) => json_response(
            &EngineDescriptorResponse {
                descriptor: engine.descriptor(),
            },
            200,
            &id,
        ),
        None => problem_response(
            ApiError::new(
                ErrorCode::UnknownEngine,
                format!("unknown engine: {engine_id}"),
            ),
            &req.path(),
            &id,
        ),
    }
}

async fn engine_search(req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let mut request = match request_from_url(&req, false) {
        Ok(request) => request,
        Err(error) => return problem_response(error, &req.path(), &request_id()),
    };
    let engine_id = ctx.param("engine_id").cloned().unwrap_or_default();
    request.engines = vec![engine_id];
    handle_search_request(req, ctx, request, true).await
}

async fn searx_compat(req: Request, ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let id = request_id();
    let request = match request_from_url(&req, true) {
        Ok(request) => request,
        Err(error) => return problem_response(error, &req.path(), &id),
    };
    let query = match ValidatedSearchRequest::try_from(request) {
        Ok(validated) => validated.0,
        Err(error) => return problem_response(error, &req.path(), &id),
    };
    match execute_search(query.clone(), &ctx, &id).await {
        Ok(response) => {
            let compatibility = SearxCompatResponse {
                query: response.query,
                number_of_results: response.result_count,
                results: response.results,
                answers: Vec::new(),
                corrections: Vec::new(),
                suggestions: Vec::new(),
                unresponsive_engines: response
                    .engines
                    .into_iter()
                    .filter_map(|report| {
                        report
                            .failure_kind
                            .map(|failure| (report.engine_id, failure))
                    })
                    .collect(),
            };
            json_response(&compatibility, 200, &id)
        }
        Err(error) => problem_response(error, &req.path(), &id),
    }
}

async fn not_found(req: Request, _ctx: RouteContext<AppData>) -> worker::Result<Response> {
    let id = request_id();
    problem_response(
        ApiError::new(ErrorCode::InvalidRequest, "route not found"),
        &req.path(),
        &id,
    )
}
