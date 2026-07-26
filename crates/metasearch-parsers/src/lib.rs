mod arxiv;
mod embedded;
mod html;

pub use arxiv::{parse_arxiv_atom, ArxivRecord};
pub use embedded::extract_embedded_json;
pub use html::{parse_selector_results, ParsedHtmlResult, SelectorResultSpec};

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserError {
    InvalidUtf8,
    InvalidXml(String),
    InvalidHtml(String),
    InvalidUrl(String),
    ProviderError(String),
    SourceTooLarge,
    NestingTooDeep,
    EmbeddedDataNotFound,
    InvalidEmbeddedData,
}

impl Display for ParserError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUtf8 => formatter.write_str("response is not valid UTF-8"),
            Self::InvalidXml(message) => write!(formatter, "invalid XML: {message}"),
            Self::InvalidHtml(message) => write!(formatter, "invalid HTML: {message}"),
            Self::InvalidUrl(message) => write!(formatter, "invalid URL: {message}"),
            Self::ProviderError(message) => write!(formatter, "provider error: {message}"),
            Self::SourceTooLarge => {
                formatter.write_str("embedded source exceeds the configured limit")
            }
            Self::NestingTooDeep => {
                formatter.write_str("embedded source nesting exceeds the configured limit")
            }
            Self::EmbeddedDataNotFound => {
                formatter.write_str("embedded data marker was not found")
            }
            Self::InvalidEmbeddedData => {
                formatter.write_str("embedded data is incomplete or invalid")
            }
        }
    }
}

impl std::error::Error for ParserError {}
