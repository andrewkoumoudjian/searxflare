use std::fmt::{Display, Formatter};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryNormalizationError {
    Empty,
    TooLong,
    ControlCharacter,
}

impl Display for QueryNormalizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("query must not be empty"),
            Self::TooLong => formatter.write_str("query must contain at most 499 characters"),
            Self::ControlCharacter => {
                formatter.write_str("query must not contain control characters")
            }
        }
    }
}

impl std::error::Error for QueryNormalizationError {}

pub fn normalize_query(input: &str) -> Result<String, QueryNormalizationError> {
    if input.chars().any(char::is_control) {
        return Err(QueryNormalizationError::ControlCharacter);
    }

    let nfc: String = input.nfc().collect();
    let trimmed = nfc.trim();
    if trimmed.is_empty() {
        return Err(QueryNormalizationError::Empty);
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut in_quotes = false;
    let mut escaped = false;
    let mut pending_space = false;

    for character in trimmed.chars() {
        if character == '"' && !escaped {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
                pending_space = false;
            }
            in_quotes = !in_quotes;
            normalized.push(character);
        } else if character.is_whitespace() && !in_quotes {
            pending_space = true;
        } else {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
                pending_space = false;
            }
            normalized.push(character);
        }

        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }

    if normalized.chars().count() > 499 {
        return Err(QueryNormalizationError::TooLong);
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_collapses_and_normalizes_unicode() {
        assert_eq!(
            normalize_query("  cafe\u{301}   rust  ").unwrap(),
            "café rust"
        );
    }

    #[test]
    fn preserves_quoted_whitespace_and_operators() {
        assert_eq!(
            normalize_query("site:example.com   \"exact   phrase\" -draft").unwrap(),
            "site:example.com \"exact   phrase\" -draft"
        );
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(
            normalize_query("hello\nworld"),
            Err(QueryNormalizationError::ControlCharacter)
        );
    }

    #[test]
    fn rejects_too_long_queries() {
        let query = "x".repeat(500);
        assert_eq!(
            normalize_query(&query),
            Err(QueryNormalizationError::TooLong)
        );
    }
}
