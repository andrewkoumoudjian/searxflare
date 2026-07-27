#![no_main]

use libfuzzer_sys::fuzz_target;
use metasearch_parsers::{parse_selector_results, SelectorResultSpec};
use url::Url;

const SPEC: SelectorResultSpec = SelectorResultSpec {
    no_results: Some(".no-results"),
    item: ".result",
    title: "h2",
    url: "a",
    description: Some(".description"),
    thumbnail: Some("img"),
};

fuzz_target!(|input: &[u8]| {
    if input.len() <= 2 * 1024 * 1024 {
        let base = Url::parse("https://example.com/search").unwrap();
        let _ = parse_selector_results(input, &base, &SPEC, 20);
    }
});
