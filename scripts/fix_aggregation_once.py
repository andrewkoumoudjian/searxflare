from pathlib import Path

runtime = Path("crates/metasearch-worker/src/runtime.rs")
source = runtime.read_text()
old = '''    let mut reports = Vec::new();
    let mut normalized = Vec::new();
    let mut failures = Vec::new();
    while let Some(run) = futures.next().await {
        reports.push(run.report);
        match run.output {
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
                    failures.push(EngineFailure::new(
                        "normalization",
                        FailureKind::EngineParseFailed,
                        message,
                    ));
                } else {
                    normalized.extend(engine_results);
                }
            }
            Err(failure) => failures.push(failure),
        }
    }
    reports.sort_by(|left, right| left.engine_id.cmp(&right.engine_id));

    if normalized.is_empty() && !failures.is_empty() {
        let first = &failures[0];
        return Err(ApiError::new(
            ErrorCode::NoEngineSucceeded,
            format!(
                "all selected engines failed; first failure was {}: {}",
                failure_code(first).as_str(),
                first.message
            ),
        ));
    }
'''
new = '''    let mut reports = Vec::new();
    let mut normalized = Vec::new();
    let mut failures = Vec::new();
    let mut successful_engines = 0usize;
    while let Some(run) = futures.next().await {
        let EngineRun {
            output,
            mut report,
        } = run;
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
'''
if old not in source:
    raise SystemExit("runtime aggregation block did not match expected source")
runtime.write_text(source.replace(old, new))

tests = Path("tests/integration/worker.spec.js")
source = tests.read_text()
source = source.replace(
    "function mockProviders({ duckFailure = false, delayArxiv = false } = {}) {",
    "function mockProviders({ duckFailure = false, delayArxiv = false, emptyWikipedia = false } = {}) {",
)
source = source.replace(
    '    if (url.hostname.endsWith("wikipedia.org")) {\n      return new Response(WIKIPEDIA, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });\n    }',
    '    if (url.hostname.endsWith("wikipedia.org")) {\n      const body = emptyWikipedia ? "<!doctype html><html><body><ul class=\\"mw-search-results\\"></ul></body></html>" : WIKIPEDIA;\n      return new Response(body, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });\n    }',
)
marker = '  it("returns NO_ENGINE_SUCCEEDED when the only engine times out", async () => {\n'
test = '''  it("keeps an empty successful engine as a partial response", async () => {
    mockProviders({ duckFailure: true, emptyWikipedia: true });
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=missing&engines=wikipedia,duckduckgo-html", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(true);
    expect(body.results).toEqual([]);
    expect(body.engines.find((engine) => engine.engine_id === "wikipedia").failure_kind).toBeUndefined();
    expect(body.engines.find((engine) => engine.engine_id === "duckduckgo-html").failure_kind).toBe("ENGINE_CHALLENGED");
  });

'''
if marker not in source:
    raise SystemExit("integration test insertion marker not found")
tests.write_text(source.replace(marker, test + marker))
