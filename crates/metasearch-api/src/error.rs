use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidRequest,
    AuthenticationRequired,
    RateLimited,
    UnknownEngine,
    EngineDisabled,
    UnsupportedCapability,
    EngineTimeout,
    EngineRateLimited,
    EngineChallenged,
    EngineAccessDenied,
    EngineResponseTooLarge,
    EngineInvalidContentType,
    EngineParseFailed,
    NoEngineSucceeded,
    InvalidCursor,
    InternalError,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::AuthenticationRequired => "AUTHENTICATION_REQUIRED",
            Self::RateLimited => "RATE_LIMITED",
            Self::UnknownEngine => "UNKNOWN_ENGINE",
            Self::EngineDisabled => "ENGINE_DISABLED",
            Self::UnsupportedCapability => "UNSUPPORTED_CAPABILITY",
            Self::EngineTimeout => "ENGINE_TIMEOUT",
            Self::EngineRateLimited => "ENGINE_RATE_LIMITED",
            Self::EngineChallenged => "ENGINE_CHALLENGED",
            Self::EngineAccessDenied => "ENGINE_ACCESS_DENIED",
            Self::EngineResponseTooLarge => "ENGINE_RESPONSE_TOO_LARGE",
            Self::EngineInvalidContentType => "ENGINE_INVALID_CONTENT_TYPE",
            Self::EngineParseFailed => "ENGINE_PARSE_FAILED",
            Self::NoEngineSucceeded => "NO_ENGINE_SUCCEEDED",
            Self::InvalidCursor => "INVALID_CURSOR",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    pub const fn status(self) -> u16 {
        match self {
            Self::InvalidRequest | Self::InvalidCursor => 400,
            Self::AuthenticationRequired => 401,
            Self::RateLimited | Self::EngineRateLimited => 429,
            Self::UnknownEngine => 404,
            Self::EngineDisabled | Self::UnsupportedCapability => 422,
            Self::EngineAccessDenied => 403,
            Self::EngineTimeout => 504,
            Self::EngineChallenged
            | Self::EngineResponseTooLarge
            | Self::EngineInvalidContentType
            | Self::EngineParseFailed
            | Self::NoEngineSucceeded => 502,
            Self::InternalError => 500,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::InvalidRequest => "Invalid request",
            Self::AuthenticationRequired => "Authentication required",
            Self::RateLimited => "Rate limited",
            Self::UnknownEngine => "Unknown engine",
            Self::EngineDisabled => "Engine disabled",
            Self::UnsupportedCapability => "Unsupported capability",
            Self::EngineTimeout => "Engine timed out",
            Self::EngineRateLimited => "Engine rate limited",
            Self::EngineChallenged => "Engine challenged",
            Self::EngineAccessDenied => "Engine access denied",
            Self::EngineResponseTooLarge => "Engine response too large",
            Self::EngineInvalidContentType => "Engine returned invalid content",
            Self::EngineParseFailed => "Engine response parsing failed",
            Self::NoEngineSucceeded => "No engine succeeded",
            Self::InvalidCursor => "Invalid cursor",
            Self::InternalError => "Internal error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldViolation {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: ErrorCode,
    pub detail: String,
    pub violations: Vec<FieldViolation>,
}

impl ApiError {
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            violations: Vec::new(),
        }
    }

    pub fn invalid(violations: Vec<FieldViolation>) -> Self {
        Self {
            code: ErrorCode::InvalidRequest,
            detail: "The request failed validation".into(),
            violations,
        }
    }

    pub fn problem(&self, instance: &str, request_id: &str) -> ProblemDetails {
        ProblemDetails {
            type_uri: format!(
                "https://searxflare.dev/problems/{}",
                self.code.as_str().to_ascii_lowercase().replace('_', "-")
            ),
            title: self.code.title().into(),
            status: self.code.status(),
            detail: self.detail.clone(),
            instance: instance.into(),
            code: self.code,
            request_id: request_id.into(),
            errors: self.violations.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub instance: String,
    pub code: ErrorCode,
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<FieldViolation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_rfc_problem_shape() {
        let problem =
            ApiError::new(ErrorCode::UnknownEngine, "missing").problem("/v1/engines/nope", "req-1");
        let value = serde_json::to_value(problem).unwrap();
        assert_eq!(
            value["type"],
            "https://searxflare.dev/problems/unknown-engine"
        );
        assert_eq!(value["status"], 404);
        assert_eq!(value["code"], "UNKNOWN_ENGINE");
    }
}
