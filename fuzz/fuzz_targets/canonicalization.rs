#![no_main]

use libfuzzer_sys::fuzz_target;
use metasearch_core::canonicalize_url;

fuzz_target!(|input: &str| {
    if input.len() <= 8 * 1024 {
        let _ = canonicalize_url(input);
    }
});
