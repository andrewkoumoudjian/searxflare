use metasearch_parsers::{parse_arxiv_atom, parse_selector_results, SelectorResultSpec};
use url::Url;

const WIKIPEDIA: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".mw-search-nonefound"),
    item: ".mw-search-result",
    title: ".mw-search-result-heading a",
    url: ".mw-search-result-heading a",
    description: Some(".searchresult"),
    thumbnail: Some(".searchResultImage img"),
};

const DUCKDUCKGO: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results, .result--no-result"),
    item: "#links .web-result",
    title: "h2 a",
    url: "h2 a",
    description: Some("a.result__snippet"),
    thumbnail: None,
};

#[test]
fn arxiv_normal_and_empty_fixtures() {
    let normal = parse_arxiv_atom(include_bytes!("../../../fixtures/engines/arxiv/normal.xml")).unwrap();
    assert_eq!(normal.len(), 1);
    assert_eq!(normal[0].authors, vec!["Ada Example"]);
    let empty = parse_arxiv_atom(include_bytes!("../../../fixtures/engines/arxiv/empty.xml")).unwrap();
    assert!(empty.is_empty());
}

#[test]
fn arxiv_truncated_fixture_fails() {
    assert!(parse_arxiv_atom(include_bytes!("../../../fixtures/engines/arxiv/truncated.xml")).is_err());
}

#[test]
fn wikipedia_normal_empty_and_changed_layout() {
    let base = Url::parse("https://en.wikipedia.org/w/index.php").unwrap();
    let normal = parse_selector_results(include_bytes!("../../../fixtures/engines/wikipedia/normal.html"), &base, &WIKIPEDIA, 10).unwrap();
    assert_eq!(normal[0].title, "Cloudflare");
    let empty = parse_selector_results(include_bytes!("../../../fixtures/engines/wikipedia/empty.html"), &base, &WIKIPEDIA, 10).unwrap();
    assert!(empty.is_empty());
    assert!(parse_selector_results(include_bytes!("../../../fixtures/engines/wikipedia/changed-layout.html"), &base, &WIKIPEDIA, 10).is_err());
}

#[test]
fn duckduckgo_normal_empty_and_changed_layout() {
    let base = Url::parse("https://html.duckduckgo.com/html/").unwrap();
    let normal = parse_selector_results(include_bytes!("../../../fixtures/engines/duckduckgo-html/normal.html"), &base, &DUCKDUCKGO, 10).unwrap();
    assert_eq!(normal.len(), 1);
    let empty = parse_selector_results(include_bytes!("../../../fixtures/engines/duckduckgo-html/empty.html"), &base, &DUCKDUCKGO, 10).unwrap();
    assert!(empty.is_empty());
    assert!(parse_selector_results(include_bytes!("../../../fixtures/engines/duckduckgo-html/changed-layout.html"), &base, &DUCKDUCKGO, 10).is_err());
}
