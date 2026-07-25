# Security assumptions

The service is designed for machine clients with bearer authentication. API keys must be long, random and rotated through Worker secrets. The Worker stores only the SHA-256 hash and compares calculated hashes in constant time.

The main SSRF boundary is the compile-time engine descriptor. Request values can alter encoded query/form values but cannot alter scheme or host. Redirects are manual and every destination is revalidated. Cross-host redirects lose cookies and sensitive headers.

Provider HTML/XML/JSON is untrusted. Parsing is bounded by source length, body size, nesting, selectors and deadlines. No scripts are executed and no external assets are loaded.

Remaining operational risks include provider blocks of Cloudflare egress, provider layout changes, local-per-PoP cache inconsistency and abuse of the unauthenticated compatibility route. Apply an edge rate limit to `/search` or require authentication before public exposure.
