# Known limitations

- DuckDuckGo HTML supports only the first page; VQD continuation is out of scope.
- Brave Web and Qwant Web use provider-controlled public frontend contracts rather than supported public APIs. Selector or schema changes may disable either engine until fixtures and parsers are updated.
- Brave Web is capped at ten pages and Qwant Web at five pages. Qwant Web does not expose a stable time-range parameter.
- Qwant DataDome cookie persistence is not implemented. Cloudflare preview egress was challenged during validation on 2026-07-25, so Qwant remains stateless and default-disabled. Explicit requests classify the challenge without bypassing it.
- PubMed, Semantic Scholar and Crossref are default-disabled and must be selected explicitly. They use public provider capacity and can independently rate-limit or deny Worker egress.
- PubMed uses a shared-deadline two-request ESearch-to-ESummary flow. The current result content uses summary and journal metadata rather than fetching full abstracts through a third request.
- Semantic Scholar uses its unauthenticated shared rate pool and caps relevance paging at the first 1,000 results. Provider API-key support is not yet wired into the engine boundary.
- Crossref requests do not yet include an operator `mailto` parameter because no service contact address is configured.
- OpenAlex is deferred because its current API requires a key and the engine boundary does not yet expose provider-scoped secrets.
- Wikipedia locale support is currently restricted to English, French, German and Spanish compile-time hosts.
- Search cursors have a signing interface but no public opaque-cursor flow yet.
- Provider parse duration and response byte measures are approximated in the current slices.
- Analytics Engine and KV are optional and not configured by default.
- Cache API is local to the serving PoP and is not globally coherent.
- Provider availability from local Wrangler does not prove availability from production Worker egress.
- A successful dry-run or preview deployment does not prove production account bindings or authenticated live search.
- There is no frontend, image/video search, browser rendering, paid API integration, proxy rotation or CAPTCHA solving.
