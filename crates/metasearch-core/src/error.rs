use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureKind {
    EngineDisabled,
    UnsupportedCapability,
    EngineTimeout,
    EngineRateLimited,
    EngineChallenged,
    EngineAccessDenied,
    EngineResponseTooLarge,
    EngineInvalidContentType,
    EngineParseFailed,
    InvalidRequest,
    Internal,
}

impl FailureKind {
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::EngineDisabled => "ENGINE_DISABLED",
            Self::UnsupportedCapability => "UNSUPPORTED_CAPABILITY",
            Self::EngineTimeout => "ENGINE_TIMEOUT",
            Self::EngineRateLimited => "ENGINE_RATE_LIMITED",
            Self::EngineChallenged => "ENGINE_CHALLENGED",
            Self::EngineAccessDenied => "ENGINE_ACCESS_DENIED",
            Self::EngineResponseTooLarge => "ENGINE_RESPONSE_TOO_LARGE",
            Self::EngineInvalidContentType => "ENGINE_INVALID_CONTENT_TYPE",
            Self::EngineParseFailed => "ENGINE_PARSE_FAILED",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::Internal => "INTERNAL_ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineFailure {
    pub engine_id: String,
    pub kind: FailureKind,
    pub message: String,
    pub retryable: bool,
}

impl EngineFailure {
    pub fn new(
        engine_id: impl Into<String>,
        kind: FailureKind,
        message: impl Into<String>,
    ) -> Self {
        let retryable = matches!(
            kind,
            FailureKind::EngineTimeout | FailureKind::EngineRateLimited
        );
        Self {
            engine_id: engine_id.into(),
            kind,
            message: message.into(),
            retryable,
        }
    }

    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

impl Display for EngineFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} failed with {}: {}",
            self.engine_id,
            self.kind.as_code(),
            self.message
        )
    }
}

impl std::error::Error for EngineFailure {}
