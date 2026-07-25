# Security contract

Trust boundaries are the authenticated API client, the Worker, Cloudflare bindings and fixed public providers.

- All `/v1/*` routes require a bearer key whose SHA-256 hash is stored in `API_KEY_SHA256`; comparison is constant-time.
- Health routes and the SearXNG compatibility route are unauthenticated by the current contract.
- Engine IDs map to compile-time code and descriptors. Clients cannot supply URLs or code.
- Outbound destinations are exact allow-listed hosts and revalidated after every redirect.
- Queries are never logged in plaintext. Logs use a SHA-256 query hash.
- Cookies, bearer credentials and provider-sensitive headers are not logged.
- Bodies, redirects, steps, engines and deadlines are bounded.
- CAPTCHA and access controls are classified, never bypassed.
- Cloudflare Access must not front the cached search route because that disables Cache API operations.

API Shield should import `spec/openapi.yaml`; start schema enforcement in log mode before blocking malformed requests.
