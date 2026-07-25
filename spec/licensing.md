# Licence and provenance

Searxflare is MIT licensed. `NOTICE` records every material upstream reference, path, reviewed revision and licence.

SearXNG is an AGPL behavioural reference. Do not paste or translate its implementation code into this repository. When adapting behaviour, write an original implementation and document endpoint/parameter provenance. Preserve any upstream header if code is ever copied under a compatible process.

Websurfx is a Rust design reference. Cloudflare `workers-rs` and `lol-html` are dependencies/references under their own licences.

Every engine PR must update:

1. `NOTICE` when a new upstream is consulted;
2. `spec/engines/<id>.yaml` with endpoint behaviour;
3. parser version when parsing changes; and
4. fixture provenance without including user data, cookies or secrets.
