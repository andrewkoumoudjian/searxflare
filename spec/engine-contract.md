# Engine contract

An engine is compiled into the binary and implements `SearchEngine` with a local (`?Send`) future. It receives only a normalised query and `EngineContext`; it never receives raw Worker `Env` access.

Each engine must:

1. Return a static descriptor with fixed hosts and limits.
2. Construct provider URLs from a fixed base URL; query input may only affect encoded path/query/form values.
3. Use `EngineHttpClient` for every outbound step.
4. Stop after `max_steps`, `max_redirects` or the supplied deadline.
5. Parse only the declared content types and return normalised provider records.
6. Treat layout changes as `ENGINE_PARSE_FAILED`, not as an empty result page.
7. Never retry a challenge, CAPTCHA, 403 or 429 response immediately.
8. Include a parser-version change whenever selector or response-model semantics change.

The registry is compile-time only. Runtime URLs, scripts and user-defined engine code are forbidden.
