use crate::ParserError;
use lol_html::{element, end_tag, text, HtmlRewriter, Settings};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use url::Url;

#[derive(Debug, Clone, Copy)]
pub struct SelectorResultSpec {
    pub no_results: Option<&'static str>,
    pub item: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    pub description: Option<&'static str>,
    pub thumbnail: Option<&'static str>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedHtmlResult {
    pub title: String,
    pub url: String,
    pub description: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Default)]
struct Builder {
    title: String,
    url: String,
    description: String,
    thumbnail: Option<String>,
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn descendant(item: &str, selector: &str) -> String {
    item.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| format!("{item} {selector}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn parse_selector_results(
    html: &[u8],
    base_url: &Url,
    spec: &SelectorResultSpec,
    max_results: usize,
) -> Result<Vec<ParsedHtmlResult>, ParserError> {
    let builders = Rc::new(RefCell::new(Vec::<Builder>::new()));
    let current = Rc::new(Cell::new(None::<usize>));
    let no_results = Rc::new(Cell::new(false));

    let mut settings = Settings::new();

    {
        let builders = Rc::clone(&builders);
        let current = Rc::clone(&current);
        settings = settings.append_element_content_handler(element!(spec.item, move |element| {
            let mut builders = builders.borrow_mut();
            if builders.len() >= max_results {
                current.set(None);
                return Ok(());
            }
            let index = builders.len();
            builders.push(Builder::default());
            current.set(Some(index));
            let current_for_end = Rc::clone(&current);
            element.on_end_tag(end_tag!(move |_| {
                if current_for_end.get() == Some(index) {
                    current_for_end.set(None);
                }
                Ok(())
            }))
        }));
    }

    {
        let builders = Rc::clone(&builders);
        let current = Rc::clone(&current);
        settings = settings.append_element_content_handler(text!(
            descendant(spec.item, spec.title),
            move |chunk| {
                if let Some(index) = current.get() {
                    if let Some(builder) = builders.borrow_mut().get_mut(index) {
                        builder.title.push_str(chunk.as_str());
                    }
                }
                Ok(())
            }
        ));
    }

    {
        let builders = Rc::clone(&builders);
        let current = Rc::clone(&current);
        settings = settings.append_element_content_handler(element!(
            descendant(spec.item, spec.url),
            move |element| {
                if let Some(index) = current.get() {
                    if let Some(builder) = builders.borrow_mut().get_mut(index) {
                        if builder.url.is_empty() {
                            builder.url = element
                                .get_attribute("href")
                                .or_else(|| element.get_attribute("data-href"))
                                .unwrap_or_default();
                        }
                    }
                }
                Ok(())
            }
        ));
    }

    if let Some(description_selector) = spec.description {
        let builders = Rc::clone(&builders);
        let current = Rc::clone(&current);
        settings = settings.append_element_content_handler(text!(
            descendant(spec.item, description_selector),
            move |chunk| {
                if let Some(index) = current.get() {
                    if let Some(builder) = builders.borrow_mut().get_mut(index) {
                        builder.description.push_str(chunk.as_str());
                    }
                }
                Ok(())
            }
        ));
    }

    if let Some(thumbnail_selector) = spec.thumbnail {
        let builders = Rc::clone(&builders);
        let current = Rc::clone(&current);
        settings = settings.append_element_content_handler(element!(
            descendant(spec.item, thumbnail_selector),
            move |element| {
                if let Some(index) = current.get() {
                    if let Some(builder) = builders.borrow_mut().get_mut(index) {
                        if builder.thumbnail.is_none() {
                            builder.thumbnail = element
                                .get_attribute("src")
                                .or_else(|| element.get_attribute("data-src"));
                        }
                    }
                }
                Ok(())
            }
        ));
    }

    if let Some(no_results_selector) = spec.no_results {
        let no_results = Rc::clone(&no_results);
        settings =
            settings.append_element_content_handler(element!(no_results_selector, move |_| {
                no_results.set(true);
                Ok(())
            }));
    }

    let mut output = Vec::new();
    let mut rewriter = HtmlRewriter::new(settings, |chunk: &[u8]| output.extend_from_slice(chunk));
    for chunk in html.chunks(8 * 1024) {
        rewriter
            .write(chunk)
            .map_err(|error| ParserError::InvalidHtml(error.to_string()))?;
    }
    rewriter
        .end()
        .map_err(|error| ParserError::InvalidHtml(error.to_string()))?;

    let mut results = Vec::new();
    for builder in builders.borrow().iter() {
        let title = collapse_whitespace(&builder.title);
        if title.is_empty() || builder.url.trim().is_empty() {
            continue;
        }
        let resolved = base_url
            .join(builder.url.trim())
            .map_err(|error| ParserError::InvalidUrl(error.to_string()))?;
        let description = collapse_whitespace(&builder.description);
        let thumbnail = builder
            .thumbnail
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| base_url.join(value).map(|url| url.to_string()))
            .transpose()
            .map_err(|error| ParserError::InvalidUrl(error.to_string()))?;
        results.push(ParsedHtmlResult {
            title,
            url: resolved.to_string(),
            description: (!description.is_empty()).then_some(description),
            thumbnail,
        });
    }

    if results.is_empty() && !no_results.get() {
        return Err(ParserError::InvalidHtml(
            "no result items matched the configured selectors".into(),
        ));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_optional_fields_and_resolves_urls() {
        let html = br#"
          <div id="links">
            <div class="web-result">
              <h2><a href="/item?a=1"> A <b>result</b> </a></h2>
              <a class="snippet">Useful <em>description</em></a>
            </div>
          </div>
        "#;
        let results = parse_selector_results(
            html,
            &Url::parse("https://example.com/search").unwrap(),
            &SelectorResultSpec {
                no_results: Some(".no-results"),
                item: "#links .web-result",
                title: "h2 a",
                url: "h2 a",
                description: Some(".snippet"),
                thumbnail: None,
            },
            10,
        )
        .unwrap();
        assert_eq!(results[0].title, "A result");
        assert_eq!(results[0].url, "https://example.com/item?a=1");
        assert_eq!(
            results[0].description.as_deref(),
            Some("Useful description")
        );
    }

    #[test]
    fn accepts_documented_empty_page() {
        let results = parse_selector_results(
            br#"<div class="no-results">Nothing found</div>"#,
            &Url::parse("https://example.com/").unwrap(),
            &SelectorResultSpec {
                no_results: Some(".no-results"),
                item: ".result",
                title: "a",
                url: "a",
                description: None,
                thumbnail: None,
            },
            10,
        )
        .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn scopes_descendants_for_each_comma_separated_item_selector() {
        let results = parse_selector_results(
            br#"
              <div class="result">
                <h3><a href="/one">First title</a></h3>
                <p class="summary">First summary</p>
              </div>
              <article class="result">
                <h3><a href="/two">Second title</a></h3>
                <p class="summary">Second summary</p>
              </article>
            "#,
            &Url::parse("https://example.com/search").unwrap(),
            &SelectorResultSpec {
                no_results: None,
                item: "div.result, article.result",
                title: "h3 a",
                url: "h3 a",
                description: Some(".summary"),
                thumbnail: None,
            },
            10,
        )
        .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "First title");
        assert_eq!(results[0].description.as_deref(), Some("First summary"));
        assert_eq!(results[1].title, "Second title");
        assert_eq!(results[1].description.as_deref(), Some("Second summary"));
    }
}
