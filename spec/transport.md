# Transport

Production outbound requests use Workers Fetch through `WorkerFetchClient`.

Before every request and redirect, the client enforces HTTPS, rejects URL credentials, checks the exact compile-time host allow-list, uses explicit cookies, and prevents query input from selecting a host. Redirects are manual, limited to two by default, and revalidated. Cookies plus authorization and proxy-authorization headers are removed when a redirect crosses hosts.

Fetches use an AbortController tied to the engine/overall deadline. Response streams are read incrementally and stopped at the descriptor body limit. Content type is validated, and challenge/access-denial classification runs before provider parsing. Challenge, CAPTCHA, 403 and 429 responses are never retried immediately.

The client does not expose sockets, TLS configuration, connection pools, proxy settings, compression controls or arbitrary request URLs.
