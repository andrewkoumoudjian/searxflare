use crate::{ApiError, FieldViolation};
use metasearch_core::{
    normalize_query, NormalizedQuery, RankingStrategy, SafeSearch, TimeRange,
    DEFAULT_OVERALL_TIMEOUT_MS, DEFAULT_RESULT_LIMIT, MAX_CATEGORY_COUNT, MAX_ENGINE_COUNT,
    MAX_OVERALL_TIMEOUT_MS, MAX_RESULT_LIMIT,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchRequest {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub engines: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<u8>,
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub safe_search: Option<SafeSearch>,
    #[serde(default)]
    pub time_range: Option<TimeRange>,
    #[serde(default)]
    pub ranking: Option<RankingStrategy>,
    #[serde(default)]
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSearchRequest(pub NormalizedQuery);

fn clean_list(values: Vec<String>) -> Vec<String> {
    let mut cleaned: Vec<String> = values
        .into_iter()
        .flat_map(|value| value.split(',').map(str::trim).filter(|part| !part.is_empty()).map(str::to_owned).collect::<Vec<_>>())
        .collect();
    cleaned.sort();
    cleaned.dedup();
    cleaned
}

impl TryFrom<SearchRequest> for ValidatedSearchRequest {
    type Error = ApiError;

    fn try_from(request: SearchRequest) -> Result<Self, Self::Error> {
        let mut violations = Vec::new();
        if request.query.is_some() && request.q.is_some() {
            violations.push(FieldViolation { field: "query".into(), message: "provide either query or q, not both".into() });
        }

        let raw_query = request.query.or(request.q).unwrap_or_default();
        let text = match normalize_query(&raw_query) {
            Ok(query) => query,
            Err(error) => {
                violations.push(FieldViolation { field: "query".into(), message: error.to_string() });
                String::new()
            }
        };

        let engines = clean_list(request.engines);
        if engines.len() > MAX_ENGINE_COUNT {
            violations.push(FieldViolation { field: "engines".into(), message: format!("at most {MAX_ENGINE_COUNT} engines may be selected") });
        }

        let categories = clean_list(request.categories);
        if categories.len() > MAX_CATEGORY_COUNT {
            violations.push(FieldViolation { field: "categories".into(), message: format!("at most {MAX_CATEGORY_COUNT} categories may be selected") });
        }

        if request.page.is_some() && request.cursor.is_some() {
            violations.push(FieldViolation { field: "page".into(), message: "page and cursor are mutually exclusive".into() });
        }
        if request.page == Some(0) {
            violations.push(FieldViolation { field: "page".into(), message: "page must be at least 1".into() });
        }

        let limit = request.limit.unwrap_or(DEFAULT_RESULT_LIMIT);
        if limit == 0 || limit > MAX_RESULT_LIMIT {
            violations.push(FieldViolation { field: "limit".into(), message: format!("limit must be between 1 and {MAX_RESULT_LIMIT}") });
        }

        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_OVERALL_TIMEOUT_MS);
        if timeout_ms == 0 || timeout_ms > MAX_OVERALL_TIMEOUT_MS {
            violations.push(FieldViolation { field: "timeout_ms".into(), message: format!("timeout_ms must be between 1 and {MAX_OVERALL_TIMEOUT_MS}") });
        }

        if !violations.is_empty() {
            return Err(ApiError::invalid(violations));
        }

        Ok(Self(NormalizedQuery {
            text,
            engines,
            categories,
            page: request.page.or(Some(1)).filter(|_| request.cursor.is_none()),
            cursor: request.cursor,
            limit,
            locale: request.locale.filter(|value| !value.trim().is_empty()),
            country: request.country.filter(|value| !value.trim().is_empty()),
            safe_search: request.safe_search.unwrap_or_default(),
            time_range: request.time_range,
            ranking: request.ranking.unwrap_or_default(),
            timeout_ms,
        }))
    }
}

impl From<ValidatedSearchRequest> for NormalizedQuery {
    fn from(request: ValidatedSearchRequest) -> Self { request.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_defaults_and_alias() {
        let validated = ValidatedSearchRequest::try_from(SearchRequest { q: Some("  cloudflare   rust ".into()), ..SearchRequest::default() }).unwrap();
        assert_eq!(validated.0.text, "cloudflare rust");
        assert_eq!(validated.0.limit, 10);
        assert_eq!(validated.0.timeout_ms, 5_000);
    }

    #[test]
    fn rejects_page_and_cursor() {
        let error = ValidatedSearchRequest::try_from(SearchRequest { query: Some("rust".into()), page: Some(2), cursor: Some("cursor".into()), ..SearchRequest::default() }).unwrap_err();
        assert_eq!(error.violations[0].field, "page");
    }

    #[test]
    fn rejects_more_than_five_engines() {
        let error = ValidatedSearchRequest::try_from(SearchRequest { query: Some("rust".into()), engines: (0..6).map(|index| format!("engine-{index}")).collect(), ..SearchRequest::default() }).unwrap_err();
        assert_eq!(error.violations[0].field, "engines");
    }
}
