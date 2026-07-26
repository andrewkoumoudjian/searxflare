use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BangTarget {
    Engine(&'static str),
    Category(&'static str),
    Profile(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BangSpec {
    alias: &'static str,
    target: BangTarget,
}

const BOOKS_PROFILE: &[&str] = &["crossref"];

const BANG_REGISTRY: &[BangSpec] = &[
    BangSpec {
        alias: "general",
        target: BangTarget::Category("general"),
    },
    BangSpec {
        alias: "web",
        target: BangTarget::Category("general"),
    },
    BangSpec {
        alias: "books",
        target: BangTarget::Profile(BOOKS_PROFILE),
    },
    BangSpec {
        alias: "arxiv",
        target: BangTarget::Engine("arxiv"),
    },
    BangSpec {
        alias: "ax",
        target: BangTarget::Engine("arxiv"),
    },
    BangSpec {
        alias: "wikipedia",
        target: BangTarget::Engine("wikipedia"),
    },
    BangSpec {
        alias: "wp",
        target: BangTarget::Engine("wikipedia"),
    },
    BangSpec {
        alias: "ddg",
        target: BangTarget::Engine("duckduckgo-html"),
    },
    BangSpec {
        alias: "brave",
        target: BangTarget::Engine("brave-web"),
    },
    BangSpec {
        alias: "qwant",
        target: BangTarget::Engine("qwant-web"),
    },
    BangSpec {
        alias: "qw",
        target: BangTarget::Engine("qwant-web"),
    },
    BangSpec {
        alias: "mojeek",
        target: BangTarget::Engine("mojeek-web"),
    },
    BangSpec {
        alias: "mj",
        target: BangTarget::Engine("mojeek-web"),
    },
    BangSpec {
        alias: "yahoo",
        target: BangTarget::Engine("yahoo-web"),
    },
    BangSpec {
        alias: "yh",
        target: BangTarget::Engine("yahoo-web"),
    },
    BangSpec {
        alias: "pubmed",
        target: BangTarget::Engine("pubmed"),
    },
    BangSpec {
        alias: "semantic-scholar",
        target: BangTarget::Engine("semantic-scholar"),
    },
    BangSpec {
        alias: "ss",
        target: BangTarget::Engine("semantic-scholar"),
    },
    BangSpec {
        alias: "crossref",
        target: BangTarget::Engine("crossref"),
    },
    BangSpec {
        alias: "cr",
        target: BangTarget::Engine("crossref"),
    },
    BangSpec {
        alias: "github",
        target: BangTarget::Engine("github"),
    },
    BangSpec {
        alias: "gh",
        target: BangTarget::Engine("github"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangResolution {
    pub original_query: String,
    pub provider_query: String,
    pub bangs: Vec<String>,
    pub engines: Vec<String>,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BangError {
    Unknown(String),
    Conflicting(String, String),
    EmptyQuery,
}

impl Display for BangError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown(bang) => write!(formatter, "unknown bang: !{bang}"),
            Self::Conflicting(left, right) => {
                write!(
                    formatter,
                    "conflicting bangs cannot be combined: !{left} and !{right}"
                )
            }
            Self::EmptyQuery => formatter.write_str("a search query cannot contain only bangs"),
        }
    }
}

impl std::error::Error for BangError {}

fn find_bang(alias: &str) -> Option<BangSpec> {
    BANG_REGISTRY
        .iter()
        .copied()
        .find(|spec| spec.alias == alias)
}

pub fn resolve_bangs(raw_query: &str) -> Result<BangResolution, BangError> {
    let original_query = raw_query.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut query_tokens = Vec::new();
    let mut bangs = Vec::new();
    let mut engines = BTreeSet::new();
    let mut categories = BTreeSet::new();
    let mut selection_kind: Option<(&'static str, String)> = None;

    for token in raw_query.split_whitespace() {
        if let Some(literal) = token.strip_prefix("\\!") {
            query_tokens.push(format!("!{literal}"));
            continue;
        }
        let Some(alias) = token.strip_prefix('!') else {
            query_tokens.push(token.to_owned());
            continue;
        };
        if alias.is_empty() {
            query_tokens.push(token.to_owned());
            continue;
        }

        let normalized = alias.to_ascii_lowercase();
        let spec = find_bang(&normalized).ok_or_else(|| BangError::Unknown(normalized.clone()))?;
        let kind = match spec.target {
            BangTarget::Engine(_) | BangTarget::Profile(_) => "engine",
            BangTarget::Category(_) => "category",
        };
        if let Some((existing_kind, existing_alias)) = &selection_kind {
            if *existing_kind != kind {
                return Err(BangError::Conflicting(existing_alias.clone(), normalized));
            }
        } else {
            selection_kind = Some((kind, normalized.clone()));
        }

        bangs.push(normalized);
        match spec.target {
            BangTarget::Engine(engine) => {
                engines.insert(engine.to_owned());
            }
            BangTarget::Profile(profile) => {
                engines.extend(profile.iter().map(|engine| (*engine).to_owned()));
            }
            BangTarget::Category(category) => {
                categories.insert(category.to_owned());
            }
        }
    }

    let provider_query = query_tokens.join(" ");
    if provider_query.trim().is_empty() {
        return Err(BangError::EmptyQuery);
    }

    Ok(BangResolution {
        original_query,
        provider_query,
        bangs,
        engines: engines.into_iter().collect(),
        categories: categories.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_category_engine_and_profile_bangs() {
        let web = resolve_bangs("!web cloudflare rust").unwrap();
        assert_eq!(web.provider_query, "cloudflare rust");
        assert_eq!(web.categories, vec!["general"]);

        let engines = resolve_bangs("cloudflare !wp !arxiv").unwrap();
        assert_eq!(engines.provider_query, "cloudflare");
        assert_eq!(engines.engines, vec!["arxiv", "wikipedia"]);

        let books = resolve_bangs("!books food safety").unwrap();
        assert_eq!(books.engines, vec!["crossref"]);

        let providers = resolve_bangs("!mj !yh independent search").unwrap();
        assert_eq!(providers.provider_query, "independent search");
        assert_eq!(providers.engines, vec!["mojeek-web", "yahoo-web"]);
    }

    #[test]
    fn accepts_compatible_category_aliases() {
        let resolution = resolve_bangs("!web !general cloudflare").unwrap();
        assert_eq!(resolution.categories, vec!["general"]);
        assert_eq!(resolution.bangs, vec!["web", "general"]);
    }

    #[test]
    fn rejects_conflicts_unknown_and_bang_only_queries() {
        assert!(matches!(
            resolve_bangs("!web !arxiv cloudflare"),
            Err(BangError::Conflicting(_, _))
        ));
        assert_eq!(
            resolve_bangs("!not-an-engine cloudflare"),
            Err(BangError::Unknown("not-an-engine".into()))
        );
        assert_eq!(resolve_bangs("!web"), Err(BangError::EmptyQuery));
    }

    #[test]
    fn preserves_literal_exclamation_marks() {
        let resolution = resolve_bangs(r"\!gh wow! cloudflare").unwrap();
        assert_eq!(resolution.provider_query, "!gh wow! cloudflare");
        assert!(resolution.bangs.is_empty());
    }
}
