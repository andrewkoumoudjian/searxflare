# Known limitations

- DuckDuckGo HTML supports only the first page; VQD continuation is out of scope.
- Wikipedia locale support is currently restricted to English, French, German and Spanish compile-time hosts.
- Search cursors have a signing interface but no public opaque-cursor flow yet.
- Provider parse duration and response byte measures are approximated in this first slice.
- Analytics Engine and KV are optional and not configured by default.
- Cache API is local to the serving PoP and is not globally coherent.
- Provider availability from local Wrangler does not prove availability from production Worker egress.
- A successful dry-run does not prove account bindings or a live deployment.
- There is no frontend, image/video search, browser rendering, paid API integration, proxy rotation or CAPTCHA solving.
