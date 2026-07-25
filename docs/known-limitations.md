# Known limitations

- DuckDuckGo HTML supports only the first page; VQD continuation is out of scope.
- Brave Web and Qwant Web use provider-controlled public frontend contracts rather than supported public APIs. Selector or schema changes may disable either engine until fixtures and parsers are updated.
- Brave Web is capped at ten pages and Qwant Web at five pages. Qwant Web does not expose a stable time-range parameter.
- Qwant DataDome cookie persistence is not implemented. The engine remains stateless and treats rate limits, challenges, access denial and invalid content as isolated provider failures.
- Wikipedia locale support is currently restricted to English, French, German and Spanish compile-time hosts.
- Search cursors have a signing interface but no public opaque-cursor flow yet.
- Provider parse duration and response byte measures are approximated in the current slices.
- Analytics Engine and KV are optional and not configured by default.
- Cache API is local to the serving PoP and is not globally coherent.
- Provider availability from local Wrangler does not prove availability from production Worker egress.
- A successful dry-run or preview deployment does not prove production account bindings or authenticated live search.
- There is no frontend, image/video search, browser rendering, paid API integration, proxy rotation or CAPTCHA solving.
