use std::fmt::{Display, Formatter};
use url::Url;

const TRACKING_PARAMETERS: &[&str] = &["gclid", "fbclid", "mc_cid", "mc_eid", "ref_src"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalizationError {
    InvalidUrl,
    UnsupportedScheme,
    CredentialsNotAllowed,
}

impl Display for CanonicalizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl => formatter.write_str("invalid result URL"),
            Self::UnsupportedScheme => formatter.write_str("only HTTP and HTTPS result URLs are allowed"),
            Self::CredentialsNotAllowed => formatter.write_str("result URLs must not contain credentials"),
        }
    }
}

impl std::error::Error for CanonicalizationError {}

fn is_tracking_parameter(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("utm_") || TRACKING_PARAMETERS.contains(&lower.as_str())
}

pub fn canonicalize_url(input: &str) -> Result<String, CanonicalizationError> {
    let mut url = Url::parse(input).map_err(|_| CanonicalizationError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(CanonicalizationError::UnsupportedScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CanonicalizationError::CredentialsNotAllowed);
    }

    url.set_fragment(None);
    if url.path().is_empty() {
        url.set_path("/");
    }

    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(name, _)| !is_tracking_parameter(name))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    pairs.sort();

    url.set_query(None);
    if !pairs.is_empty() {
        let mut query = url.query_pairs_mut();
        for (name, value) in pairs {
            query.append_pair(&name, &value);
        }
    }

    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fragments_tracking_and_sorts_query() {
        assert_eq!(
            canonicalize_url("https://Example.com:443/path?z=2&utm_source=x&a=1#part").unwrap(),
            "https://example.com/path?a=1&z=2"
        );
    }

    #[test]
    fn preserves_http_as_distinct() {
        assert_eq!(canonicalize_url("http://example.com").unwrap(), "http://example.com/");
    }

    #[test]
    fn rejects_credentials() {
        assert_eq!(
            canonicalize_url("https://user:pass@example.com"),
            Err(CanonicalizationError::CredentialsNotAllowed)
        );
    }
}
