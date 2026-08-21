# UA Spoof Experiment — 2026-08-21 (consolidated)

## Question

Can a Cloudflare Worker spoof `User-Agent` to get cleaner / unblocked responses from upstream search engines and public LinkedIn pages? Do strings like `Claude-User`, `Google`, `OpenAI File Downloader`, `XaiImageApiFetch/1.0`, `GPTBot/1.2`, `PerplexityBot/1.0` get special-cased?

Workers *can* set `User-Agent` on outbound `fetch()` — Cloudflare allows it, unlike browsers.

## Implementation (additive, reversible, nurau-ops-sales)

- `engine/crates/metasearch-http/src/lib.rs:45` — `WorkerFetchClient` stores `spoof_user_agent: Option<String>` with `with_spoofed_user_agent()` / `apply_spoof_to_request()` / `spoofed_user_agent()`. On `wasm32`, `send()` at `engine/crates/metasearch-http/src/lib.rs:509` calls `apply_spoof_to_request()` before `apply_bot_auth()`/`build_request()`, overriding `user-agent` and `api-user-agent` when set. Tests at `engine/crates/metasearch-http/src/lib.rs:728`.

- `engine/crates/metasearch-worker/src/runtime.rs:212` — `spoofed_user_agent(env)` reads `SPOOF_USER_AGENT` (secret or var):

```
claude        → Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Claude-User/1.0; +mailto:support@anthropic.com)
claude-user   → Claude-User
gemini|google → Google
openai        → OpenAI File Downloader
xai           → XaiImageApiFetch/1.0 (Linux; x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.3
xai-short     → XaiImageApiFetch/1.0
gptbot        → Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.2; +https://openai.com/gptbot)
perplexity    → Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)
googlebot     → Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)
mozilla|chrome→ Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36
curl          → curl/8.0
searxflare    → Searxflare/0.1 (+https://github.com/andrewkoumoudjian/searxflare)
<custom>      → verbatim
```

Wired at `engine/crates/metasearch-worker/src/runtime.rs:810` via `WorkerFetchClient::new(bot_auth).with_spoofed_user_agent(...)`. Absent/empty → no spoof (current behaviour). Documented in `engine/wrangler.jsonc:8`.

```jsonc
// engine/wrangler.jsonc
{"vars":{"ENABLE_STARTPAGE":"true","ENABLE_GOOGLE":"true"}}
// optional: "SPOOF_USER_AGENT": "claude-user" // see above
```

Unset to revert. Also `scripts/ua-compare.mjs` (`bun scripts/ua-compare.mjs <url>`).

## Harness

- Local (macOS residential, `192.168.2.236`): `node /tmp/ua_experiment.mjs` → 6 UAs, then `node /tmp/retest.mjs` → 8 exact UAs — `fetch(url,{headers:{"User-Agent":UA},redirect:"manual"})`.
- Workers: `nurau-ua-experiment` / `nurau-ua-experiment2` at `YUL` (Montreal) `https://nurau-ua-experiment*.andrew-koumoudjian.workers.dev/compare?url=...` — same `fetch` inside Worker, `colo=YUL`, `cf:{cacheTtl:0}`.
- Table Image 1/2: VPS `rt machine 🇺🇦` running identical `curl -A "$A"` loop for `Claude-User` / `GPTBot/1.2` / `PerplexityBot/1.0` / `Google` / `XaiImageApiFetch/1.0`.

## Experiment 1 — 6-UAs (Searxflare, Claude-Mozilla, OpenAI FD, XAI-short, Googlebot, Mozilla) — summary

See initial section for full table; key Worker `YUL` vs Local:

- LinkedIn `in/williamhgates`: Local `200 442-654k` (UA-dependent length, link counts `197-283`), Worker `999 1530` for *all* UAs.
- Brave `search.brave.com`: Worker all `200`, Local `OpenAI FD` only `200` then flaky.
- DDG `html.duckduckgo.com`: Only `Searxflare/0.1` `200` from Worker, others `202 14k`.

## Experiment 2 — exact UAs requested (2026-08-21 retest)

```
Claude:      Claude-User
Gemini:      Google
OpenAI FD:   OpenAI File Downloader
xAI Images:  XaiImageApiFetch/1.0 (Linux; x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.3
GPTBot:      Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; GPTBot/1.2; +https://openai.com/gptbot)
Perplexity:  Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot)
default:     curl/8.0
chrome:      Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36
```

### LinkedIn Pulse `https://www.linkedin.com/pulse/artificial-intelligence-trends-2026`

| UA | Local | Worker YUL |
|----|-------|------------|
| `Claude-User` | `404 317k LinkedIn` | `404 317k` |
| `Google` | `301 0 → /top-content` | `301 0` |
| `OpenAI FD` | `301 0` | `301 0` |
| `xAI` | `404 317k` | `404 317k` |
| `GPTBot` | `404 317k` | `404 317k` |
| `Perplexity` | `404 317k` | `404 317k` |
| `curl/8.0` | `404 317k` | `404 317k` |
| `Chrome` | `301 0` | `301 0` |

Matches Image 2 VPS: `default 404:319687`, `claude 404:319687`, `git(Google) 301:0`, `gbot 404:319687`. Article is gone — `404`/`301` everywhere, UA only swaps which.

### LinkedIn Profile `https://www.linkedin.com/in/williamhgates`

| UA | Local | Worker YUL |
|----|-------|------------|
| `Claude-User` | `200 491k ✓` | `999 1530 ×` |
| `Google` | `200 626k ✓` | `999 1530 ×` |
| `OpenAI FD` | `200 642k ✓` | `999 1530 ×` |
| `xAI` | `200 432k ✓` | `999 1530 ×` |
| `GPTBot` | `200 572k ✓` | `999 1530 ×` |
| `Perplexity` | `200 575k ✓` | `999 1530 ×` |
| `curl/8.0` | `999 1530 × wall` | `999 1530 ×` |
| `Chrome` | `200 649k ✓` | `999 1530 ×` |

Local `curl` is the only `wall` (Image 1 `LinkedIn¹ curl × wall`), all AI/Chrome `✓` locally — same as Image 1 table `LinkedIn¹ Claude ✓ / Google ✓` vs `GPTBot ×999`. From `YUL` all `×999` — Cloudflare ASN blocked.

### What `Claude-User 200 491k` actually contains (local)

```
pageKey=public_profile_v3_desktop
data-is-bot="true" (Chrome/Google is "false")
<title>Bill Gates - Gates Foundation | LinkedIn</title>
<meta name="description" content="Chair of the Gates Foundation. Founder of Breakthrough Energy. Co-founder of Microsoft… · Experience: Gates Foundation · Education: Harvard · Location: Seattle · 8 connections">
H1=Bill Gates
H2=About / Articles by Bill / Activity / Experience / Education / View Bill’s full profile
Gates Foundation ×21
application/ld+json @graph Article {headline: Small bugs, big breakthroughs, likes 2420}
visible text start: Bill Gates Seattle 41M followers 8 connections … Join to view profile
```

`curl -A "Claude-User" -H "Accept: text/html,application/xhtml+xml" -H "Accept-Language: en-US,en;q=0.8" https://www.linkedin.com/in/williamhgates -o linkedin.html` → `495179` `public_profile_v3_desktop` (your run). `grep '<title>.*</title>' linkedin.html` gives the above. Private sections still `Join to view profile` authwall — public SSR only.

### NYTimes `https://www.nytimes.com/2026/08/20/technology/ai-models.html`

| UA | Local | Worker YUL |
|----|-------|------------|
| `Claude-User`/`GPTBot`/`Perplexity` | `403 139k` | `403 139k` |
| `Google`/`OpenAI FD`/`xAI`/`curl`/`Chrome` | `403 771` | `403 771-774` |

Body-size split is the signal Image 1 encodes as `×403` vs `✓`; both are `403` but different block pages.

### Brave `search.brave.com/search?q=cloudflare+rust`

| UA | Local | Worker YUL |
|----|-------|------------|
| `Claude-User`/`Google`/`OpenAI FD`/`xAI`/`GPTBot`/`Perplexity`/`curl` | `429 73k` | `200 315k` |
| `Chrome` | `429 73k` | `429 73k` |

Worker `Chrome` alone `429` — others `200` from `YUL` (earlier experiment all `200`).

### DDG `html.duckduckgo.com/html/?q=cloudflare+rust`

| UA | Local | Worker YUL |
|----|-------|------------|
| `Google` | `200 32k ✓` | `200 33410 ✓` |
| `OpenAI FD` | `200 32k ✓` | `202 14k ×` |
| `Chrome` | `200 33k ✓` | `202 14k ×` |
| `Claude-User`/`xAI`/`GPTBot`/`Perplexity`/`curl` | `202 14k ×` | `202 14k ×` |

Only `Google` passes from Worker in this exact set; previously `Searxflare/0.1` was the only `✓` in the 6-UA set. `Claude-User` fails DDG everywhere.

### Wikipedia `en.wikipedia.org/wiki/Cloudflare`

All 8 UAs `200 769k` both envs — except `Googlebot` `403 bot-traffic@wikimedia.org` in the 6-UA set (exact `Google` is not `Googlebot` and passes).

### Startpage `www.startpage.com/`

| UA | Local | Worker YUL |
|----|-------|------------|
| `Claude-User`/`Google`/`OpenAI FD` | `200 9-10k` | `200 9-10k` |
| `Chrome` | `200 196k` | `200 10k` |
| `xAI` | `303 captcha-block` | `303 captcha-block` |
| `GPTBot`/`Perplexity` | `302 blocked.html` | `302 blocked.html` |
| `curl` | `303 captcha` | `303 captcha` |

### Qwant `www.qwant.com/?q=cloudflare`

All `200 139k` both envs, no UA sensitivity.

## Synthesis

- Workers *can* set `User-Agent` (header arrives; DDG `200` vs `202`, Brave `429` vs `200`, LinkedIn pulse `404` vs `301` prove it). Image 2's `curl -A` is the same.
- **ASN matters more than UA.** LinkedIn `999` from `YUL` for *every* UA vs `200` for `Claude-User`/`Google`/etc locally and `✓` on VPS (Image 1) — Cloudflare ranges are blocklisted; VPS `rt machine 🇺🇦` is not.
- **Bot verification is UA+IP RDNS.** `Claude-User` must RDNS → `*.anthropic.com` in published ranges; `GPTBot` → `*.openai.com`; `Google` (Gemini) → `Google-InspectionTool` verified differently. Spoofing from `workers.dev` fails RDNS.
- **No universal AI UA.** `Google` helps DDG but is `301` on LinkedIn pulse; `OpenAI FD` helps Brave locally but fails DDG from Worker; `Claude-User` helps LinkedIn profile locally but fails DDG; `xAI` fails DDG/Startpage; `GPTBot`/`Perplexity` blocked on Startpage.
- Best general Worker UA remains `Searxflare/0.1` (only `200` on all engines in 6-UA set); in exact set `Google` is DDG-best but not universal.

## Recommendation (nurau-ops-sales & searxflare)

1. Keep default (no `SPOOF_USER_AGENT`) in prod — optimal for Worker ASN.
2. Use `SPOOF_USER_AGENT` as lever for A/B: `wrangler secret put SPOOF_USER_AGENT` → `claude-user` / `google` / `openai` / `xai` / `gptbot` / `perplexity` / verbatim UA; measure `EngineExecutionReport` / `SEARCH_ANALYTICS`.
3. Don't use UA spoof for LinkedIn — require off-Worker egress: residential proxy/`cloudflared tunnel`/PhantomBuster actuator or `lane:"fiber"|"exa-people"` (`src/index.ts:77` in nurau-ops-sales, `EXA_API_KEY` in searxflare). Browser cookies never leave local device.
4. For LinkedIn `200` `495k` locally: `curl -A "Claude-User" -H "Accept: text/html,application/xhtml+xml" https://www.linkedin.com/in/williamhgates -o linkedin.html` — parse `ld+json`/`Experience`; same via `WorkerFetchClient` only works when egress is non-Cloudflare.

## Reproducing

```bash
bun scripts/ua-compare.mjs "https://www.linkedin.com/in/williamhgates"
node /tmp/linkedin_content.mjs  # Claude-User 491k parse
curl -s "https://nurau-ua-experiment2.andrew-koumoudjian.workers.dev/?url=https://html.duckduckgo.com/html/?q=cloudflare+rust" | jq .
SPOOF_USER_AGENT=google bun run build:engine
echo "claude-user" | wrangler secret put SPOOF_USER_AGENT --config engine/wrangler.jsonc
```

## Files

- `engine/crates/metasearch-http/src/lib.rs` — spoof plumbing + tests
- `engine/crates/metasearch-worker/src/runtime.rs` — `spoofed_user_agent()` resolver
- `engine/wrangler.jsonc` — optional `SPOOF_USER_AGENT`
- `scripts/ua-compare.mjs` — local harness
