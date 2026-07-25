# Engine implementation guide

1. Confirm the provider exposes a public endpoint whose use does not require bypassing access controls.
2. Add a fixed `EngineDescriptor`; never accept a host or endpoint from a request.
3. Add an engine manifest validated by `spec/engine-manifest.schema.json`.
4. Build requests only through `EngineHttpClient`, with explicit content types, headers and cookies.
5. Use `serde_json`, `quick-xml` or the selector-driven `lol-html` parser. Embedded data must use the bounded extractor.
6. Return `ProviderResult` values with one-based positions and specialist metadata.
7. Add normal, empty, changed-layout, denied, limited, challenged, truncated, oversized, wrong-content-type and redirect fixtures.
8. Unit-test parsing, URL decoding and failure classification.
9. Increment the parser version when parsing semantics change.
10. Update `NOTICE`, documentation, OpenAPI when needed and the compile-time registry.

An empty result is valid only when the response contains a provider-specific no-results marker. A selector miss without that marker is a parse failure.
