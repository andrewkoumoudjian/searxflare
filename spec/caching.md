# Caching

Cache API stores final aggregate responses, normalised per-engine outputs and short-lived negative provider failures. Keys include API version, NFC-normalised query, sorted engines/categories, page or cursor hash, limit, locale, country, safe-search, time range, registry version, parser versions and ranking version.

Default TTLs:

- general web: 180 seconds
- academic/reference: 1800 seconds
- partial aggregate: 20 seconds
- negative provider failure: 30 seconds

Writes run through `wait_until`. Cache responses are constructed internally and never contain `Set-Cookie`. Client responses remain `private, no-store`. Cache API is local to the Cloudflare PoP and must not be treated as globally coherent state.
