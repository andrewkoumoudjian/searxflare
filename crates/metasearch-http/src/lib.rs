use metasearch_core::{
    BoundedResponse, Deadline, EngineDescriptor, EngineFailure, EngineHttpClient, EngineRequest,
    FailureKind,
};
use url::Url;

const CHALLENGE_SCAN_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct BotAuthConfig {
    pub key_id: String,
    pub directory_url: String,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    private_key: [u8; 32],
}

impl BotAuthConfig {
    pub fn from_base64(
        key_id: impl Into<String>,
        directory_url: impl Into<String>,
        encoded_private_key: &str,
    ) -> Result<Self, String> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let decoded = URL_SAFE_NO_PAD
            .decode(encoded_private_key.trim())
            .map_err(|error| error.to_string())?;
        let private_key = decoded
            .try_into()
            .map_err(|_| "Web Bot Auth private key must contain exactly 32 bytes".to_string())?;
        let directory_url = directory_url.into();
        let url = Url::parse(&directory_url).map_err(|error| error.to_string())?;
        if url.scheme() != "https" || url.host_str().is_none() {
            return Err("Web Bot Auth directory URL must be an absolute HTTPS URL".into());
        }
        Ok(Self {
            key_id: key_id.into(),
            directory_url,
            private_key,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct WorkerFetchClient {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    bot_auth: Option<BotAuthConfig>,
}

impl WorkerFetchClient {
    pub const fn new(bot_auth: Option<BotAuthConfig>) -> Self {
        Self { bot_auth }
    }
}

pub fn validate_destination(
    engine: &'static EngineDescriptor,
    url: &Url,
) -> Result<(), EngineFailure> {
    if url.scheme() != "https" && !(engine.allow_http && url.scheme() == "http") {
        return Err(EngineFailure::new(
            engine.id,
            FailureKind::EngineAccessDenied,
            "outbound engine requests require HTTPS",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EngineFailure::new(
            engine.id,
            FailureKind::EngineAccessDenied,
            "outbound URLs must not contain credentials",
        ));
    }
    let host = url.host_str().ok_or_else(|| {
        EngineFailure::new(
            engine.id,
            FailureKind::EngineAccessDenied,
            "outbound URL has no host",
        )
    })?;
    if !engine
        .allowed_hosts
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
    {
        return Err(EngineFailure::new(
            engine.id,
            FailureKind::EngineAccessDenied,
            format!("outbound host {host} is not allowed for this engine"),
        ));
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
fn content_type_matches(actual: Option<&str>, accepted: &[&str]) -> bool {
    let Some(actual) = actual else { return false };
    let media_type = actual
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    accepted
        .iter()
        .any(|expected| media_type == expected.to_ascii_lowercase())
}

pub fn classify_challenge(engine_id: &str, status: u16, body: &[u8]) -> Option<EngineFailure> {
    if status == 429 {
        return Some(EngineFailure::new(
            engine_id,
            FailureKind::EngineRateLimited,
            "provider returned HTTP 429",
        ));
    }

    let scan =
        String::from_utf8_lossy(&body[..body.len().min(CHALLENGE_SCAN_LIMIT)]).to_ascii_lowercase();
    let rate_limit_markers = [
        "rate limit exceeded",
        "secondary rate limit",
        "too many requests",
        "request quota exceeded",
    ];
    if rate_limit_markers
        .iter()
        .any(|marker| scan.contains(marker))
    {
        return Some(EngineFailure::new(
            engine_id,
            FailureKind::EngineRateLimited,
            format!("provider returned rate-limit response with HTTP {status}"),
        ));
    }

    let challenge_markers = [
        "cf-chl-",
        "cloudflare ray id",
        "attention required",
        "captcha",
        "challenge-form",
        "verify you are human",
        "/sorry/",
        "unusual traffic",
    ];
    if challenge_markers.iter().any(|marker| scan.contains(marker)) {
        return Some(
            EngineFailure::new(
                engine_id,
                FailureKind::EngineChallenged,
                "provider returned a challenge or CAPTCHA page",
            )
            .with_retryable(false),
        );
    }

    let denied_markers = [
        "access denied",
        "request blocked",
        "permission denied",
        "forbidden",
    ];
    if status == 403 || denied_markers.iter().any(|marker| scan.contains(marker)) {
        return Some(
            EngineFailure::new(
                engine_id,
                FailureKind::EngineAccessDenied,
                "provider denied access",
            )
            .with_retryable(false),
        );
    }
    None
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use futures_util::{future::Either, pin_mut, TryStreamExt};
    use indexmap::IndexMap;
    use js_sys::Uint8Array;
    use metasearch_core::{BotAuthPolicy, EngineMethod};
    use std::{collections::BTreeMap, time::Duration};
    use uuid::Uuid;
    use web_bot_auth::{
        components::{CoveredComponent, DerivedComponent, HTTPField, HTTPFieldParametersSet},
        keyring::Algorithm,
        message_signatures::{MessageSigner, UnsignedMessage},
    };

    const SENSITIVE_HEADERS: &[&str] = &[
        "authorization",
        "cookie",
        "proxy-authorization",
        "signature",
        "signature-input",
        "signature-agent",
    ];

    fn strip_sensitive_headers(headers: &mut BTreeMap<String, String>) {
        headers.retain(|name, _| {
            !SENSITIVE_HEADERS
                .iter()
                .any(|sensitive| name.eq_ignore_ascii_case(sensitive))
        });
    }
    use wasm_bindgen::JsValue;
    use worker::{
        AbortController, Date, Delay, Fetch, Headers, Method, Request, RequestInit, RequestRedirect,
    };

    fn worker_error(engine_id: &str, error: impl ToString) -> EngineFailure {
        EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
    }

    fn method(method: EngineMethod) -> Method {
        match method {
            EngineMethod::Get => Method::Get,
            EngineMethod::Post => Method::Post,
        }
    }

    struct SignableRequest {
        method: String,
        authority: String,
        target_uri: String,
        signature_agent: String,
        signature_input: String,
        signature: String,
    }

    impl UnsignedMessage for SignableRequest {
        fn fetch_components_to_cover(&self) -> IndexMap<CoveredComponent, String> {
            IndexMap::from_iter([
                (
                    CoveredComponent::Derived(DerivedComponent::Method { req: false }),
                    self.method.clone(),
                ),
                (
                    CoveredComponent::Derived(DerivedComponent::Authority { req: false }),
                    self.authority.clone(),
                ),
                (
                    CoveredComponent::Derived(DerivedComponent::TargetUri { req: false }),
                    self.target_uri.clone(),
                ),
                (
                    CoveredComponent::HTTP(HTTPField {
                        name: "signature-agent".into(),
                        parameters: HTTPFieldParametersSet(Vec::new()),
                    }),
                    self.signature_agent.clone(),
                ),
            ])
        }

        fn register_header_contents(&mut self, signature_input: String, signature_header: String) {
            self.signature_input = format!("sig1={signature_input}");
            self.signature = format!("sig1={signature_header}");
        }
    }

    fn apply_bot_auth(
        client: &WorkerFetchClient,
        engine: &'static EngineDescriptor,
        request: &mut EngineRequest,
    ) -> Result<(), EngineFailure> {
        if engine.bot_auth_policy == BotAuthPolicy::Disabled {
            return Ok(());
        }
        let Some(config) = client.bot_auth.as_ref() else {
            if engine.bot_auth_policy == BotAuthPolicy::Required {
                return Err(EngineFailure::new(
                    engine.id,
                    FailureKind::EngineDisabled,
                    "Web Bot Auth is required by this engine but is not configured",
                ));
            }
            return Ok(());
        };
        let authority = request
            .url
            .host_str()
            .ok_or_else(|| {
                EngineFailure::new(
                    engine.id,
                    FailureKind::Internal,
                    "cannot sign a URL without an authority",
                )
            })?
            .to_owned();
        let signature_agent = format!("\"{}\"", config.directory_url);
        let mut signable = SignableRequest {
            method: match request.method {
                EngineMethod::Get => "GET",
                EngineMethod::Post => "POST",
            }
            .into(),
            authority,
            target_uri: request.url.to_string(),
            signature_agent: signature_agent.clone(),
            signature_input: String::new(),
            signature: String::new(),
        };
        MessageSigner {
            keyid: config.key_id.clone(),
            nonce: Uuid::new_v4().to_string(),
            tag: "web-bot-auth".into(),
        }
        .generate_signature_headers_content(
            &mut signable,
            Duration::from_secs(60),
            Algorithm::Ed25519,
            &config.private_key,
        )
        .map_err(|error| EngineFailure::new(engine.id, FailureKind::Internal, error.to_string()))?;
        request
            .headers
            .insert("signature-agent".into(), signature_agent);
        request
            .headers
            .insert("signature-input".into(), signable.signature_input);
        request
            .headers
            .insert("signature".into(), signable.signature);
        Ok(())
    }

    fn build_request(engine_id: &str, request: &EngineRequest) -> Result<Request, EngineFailure> {
        let headers = Headers::new();
        for (name, value) in &request.headers {
            headers
                .set(name, value)
                .map_err(|error| worker_error(engine_id, error))?;
        }
        if !request.cookies.is_empty() {
            let cookie = request
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ");
            headers
                .set("cookie", &cookie)
                .map_err(|error| worker_error(engine_id, error))?;
        }

        let mut init = RequestInit::new();
        init.method = method(request.method);
        init.headers = headers;
        init.redirect = RequestRedirect::Manual;
        if let Some(body) = &request.body {
            init.body = Some(JsValue::from(Uint8Array::from(body.as_slice())));
        }
        Request::new_with_init(request.url.as_str(), &init)
            .map_err(|error| worker_error(engine_id, error))
    }

    async fn fetch_with_deadline(
        engine_id: &str,
        request: Request,
        deadline: Deadline,
    ) -> Result<worker::Response, EngineFailure> {
        let now_ms = Date::now().as_millis() as u64;
        let remaining_ms = deadline.remaining_ms(now_ms);
        if remaining_ms == 0 {
            return Err(EngineFailure::new(
                engine_id,
                FailureKind::EngineTimeout,
                "engine deadline expired before fetch",
            ));
        }

        let controller = AbortController::default();
        let signal = controller.signal();
        let fetch_request = Fetch::Request(request);
        let fetch = fetch_request.send_with_signal(&signal);
        let delay = Delay::from(Duration::from_millis(remaining_ms));
        pin_mut!(fetch);
        pin_mut!(delay);
        match futures_util::future::select(fetch, delay).await {
            Either::Left((response, _)) => response.map_err(|error| {
                let message = error.to_string();
                if message.contains("AbortError") {
                    EngineFailure::new(
                        engine_id,
                        FailureKind::EngineTimeout,
                        "engine fetch aborted",
                    )
                } else {
                    worker_error(engine_id, message)
                }
            }),
            Either::Right((_, _)) => {
                controller.abort();
                Err(EngineFailure::new(
                    engine_id,
                    FailureKind::EngineTimeout,
                    "engine fetch exceeded its deadline",
                ))
            }
        }
    }

    async fn read_bounded_body(
        engine: &'static EngineDescriptor,
        response: &mut worker::Response,
    ) -> Result<Vec<u8>, EngineFailure> {
        if let Some(content_length) = response
            .headers()
            .get("content-length")
            .map_err(|error| worker_error(engine.id, error))?
            .and_then(|value| value.parse::<usize>().ok())
        {
            if content_length > engine.max_body_bytes {
                return Err(EngineFailure::new(
                    engine.id,
                    FailureKind::EngineResponseTooLarge,
                    format!("provider declared {content_length} bytes"),
                ));
            }
        }

        match response.stream() {
            Ok(mut stream) => {
                let mut body = Vec::new();
                while let Some(chunk) = stream
                    .try_next()
                    .await
                    .map_err(|error| worker_error(engine.id, error))?
                {
                    if body.len().saturating_add(chunk.len()) > engine.max_body_bytes {
                        return Err(EngineFailure::new(
                            engine.id,
                            FailureKind::EngineResponseTooLarge,
                            "provider body exceeded the configured limit",
                        ));
                    }
                    body.extend_from_slice(&chunk);
                }
                Ok(body)
            }
            Err(_) => {
                let body = response
                    .bytes()
                    .await
                    .map_err(|error| worker_error(engine.id, error))?;
                if body.len() > engine.max_body_bytes {
                    return Err(EngineFailure::new(
                        engine.id,
                        FailureKind::EngineResponseTooLarge,
                        "provider body exceeded the configured limit",
                    ));
                }
                Ok(body)
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl EngineHttpClient for WorkerFetchClient {
        async fn send(
            &self,
            engine: &'static EngineDescriptor,
            mut request: EngineRequest,
            deadline: Deadline,
        ) -> Result<BoundedResponse, EngineFailure> {
            validate_destination(engine, &request.url)?;
            let started_at = Date::now().as_millis() as u64;
            let mut redirect_count = 0u8;

            loop {
                apply_bot_auth(self, engine, &mut request)?;
                let outgoing = build_request(engine.id, &request)?;
                let mut response = fetch_with_deadline(engine.id, outgoing, deadline).await?;
                let status = response.status_code();
                let headers: BTreeMap<String, String> = response.headers().entries().collect();

                if matches!(status, 301 | 302 | 303 | 307 | 308) {
                    if redirect_count >= engine.max_redirects {
                        return Err(EngineFailure::new(
                            engine.id,
                            FailureKind::EngineAccessDenied,
                            "provider exceeded the configured redirect limit",
                        ));
                    }
                    let location = headers.get("location").ok_or_else(|| {
                        EngineFailure::new(
                            engine.id,
                            FailureKind::EngineParseFailed,
                            "redirect response omitted Location",
                        )
                    })?;
                    let next = request.url.join(location).map_err(|error| {
                        EngineFailure::new(
                            engine.id,
                            FailureKind::EngineAccessDenied,
                            error.to_string(),
                        )
                    })?;
                    validate_destination(engine, &next)?;
                    let cross_host = request.url.host_str() != next.host_str();
                    if cross_host {
                        strip_sensitive_headers(&mut request.headers);
                        request.cookies.clear();
                    }
                    if status == 303 {
                        request.method = EngineMethod::Get;
                        request.body = None;
                        request.headers.remove("content-type");
                    }
                    request.url = next;
                    redirect_count += 1;
                    continue;
                }

                let body = read_bounded_body(engine, &mut response).await?;
                if let Some(failure) = classify_challenge(engine.id, status, &body) {
                    return Err(failure);
                }
                if !(200..300).contains(&status) {
                    return Err(EngineFailure::new(
                        engine.id,
                        FailureKind::EngineAccessDenied,
                        format!("provider returned HTTP {status}"),
                    ));
                }
                let content_type = headers.get("content-type").map(String::as_str);
                let empty_acknowledgement = body.is_empty() && matches!(status, 202 | 204);
                if !empty_acknowledgement
                    && !content_type_matches(content_type, request.accepted_content_types)
                {
                    return Err(EngineFailure::new(
                        engine.id,
                        FailureKind::EngineInvalidContentType,
                        format!(
                            "unexpected content type: {}",
                            content_type.unwrap_or("missing")
                        ),
                    ));
                }

                return Ok(BoundedResponse {
                    status,
                    headers,
                    body,
                    final_url: request.url,
                    redirect_count,
                    duration_ms: (Date::now().as_millis() as u64).saturating_sub(started_at),
                });
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait(?Send)]
impl EngineHttpClient for WorkerFetchClient {
    async fn send(
        &self,
        engine: &'static EngineDescriptor,
        _request: EngineRequest,
        _deadline: Deadline,
    ) -> Result<BoundedResponse, EngineFailure> {
        Err(EngineFailure::new(
            engine.id,
            FailureKind::Internal,
            "WorkerFetchClient is only available on wasm32-unknown-unknown",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metasearch_core::{
        BotAuthPolicy, CachePolicy, EngineCapabilities, EngineMaturity, SourceKind, StatePolicy,
    };

    static DESCRIPTOR: EngineDescriptor = EngineDescriptor {
        id: "test",
        display_name: "Test",
        categories: &["general"],
        source_kind: SourceKind::Html,
        maturity: EngineMaturity::Experimental,
        allowed_hosts: &["example.com"],
        capabilities: EngineCapabilities {
            paging: false,
            locale: false,
            country: false,
            safe_search: false,
            time_range: false,
        },
        timeout_ms: 1_000,
        max_body_bytes: 1024,
        max_steps: 1,
        max_redirects: 1,
        weight: 1.0,
        parser_version: "test-v1",
        default_enabled: true,
        allow_http: false,
        state_policy: StatePolicy::Stateless,
        cache_policy: CachePolicy {
            response_ttl_seconds: 10,
            negative_ttl_seconds: 5,
        },
        bot_auth_policy: BotAuthPolicy::Disabled,
    };

    #[test]
    fn blocks_unlisted_hosts_and_credentials() {
        assert!(validate_destination(
            &DESCRIPTOR,
            &Url::parse("https://example.com/path").unwrap()
        )
        .is_ok());
        assert!(
            validate_destination(&DESCRIPTOR, &Url::parse("https://evil.test/path").unwrap())
                .is_err()
        );
        assert!(validate_destination(
            &DESCRIPTOR,
            &Url::parse("https://user@example.com/path").unwrap()
        )
        .is_err());
    }

    #[test]
    fn detects_challenges_before_parsing() {
        assert_eq!(
            classify_challenge("test", 200, b"<form id='challenge-form'>captcha</form>")
                .unwrap()
                .kind,
            FailureKind::EngineChallenged
        );
        assert_eq!(
            classify_challenge("test", 429, b"").unwrap().kind,
            FailureKind::EngineRateLimited
        );
        assert_eq!(
            classify_challenge(
                "test",
                403,
                br#"{"message":"API rate limit exceeded for this client."}"#
            )
            .unwrap()
            .kind,
            FailureKind::EngineRateLimited
        );
        assert_eq!(
            classify_challenge("test", 403, b"Forbidden").unwrap().kind,
            FailureKind::EngineAccessDenied
        );
    }

    #[test]
    fn validates_content_types_without_parameters() {
        assert!(content_type_matches(
            Some("text/html; charset=UTF-8"),
            &["text/html"]
        ));
        assert!(!content_type_matches(
            Some("application/json"),
            &["text/html"]
        ));
    }
}
