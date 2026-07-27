import { env, exports } from "cloudflare:workers";
import { createExecutionContext, waitOnExecutionContext } from "cloudflare:test";
import { afterEach, describe, expect, it, vi } from "vitest";
import WorkerEntrypoint, {
  aiChunkToResult,
  mergeRankedResults,
} from "../../crates/metasearch-worker/worker/facade.mjs";

const AUTH = { authorization: "Bearer test-api-key" };

const ARXIV = `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry><id>https://arxiv.org/abs/1234.5678v1</id><title>Cloudflare Rust</title><summary>Academic result.</summary><published>2026-01-02T03:04:05Z</published><author><name>Ada Example</name></author><link title="pdf" href="https://arxiv.org/pdf/1234.5678v1" type="application/pdf" /><category term="cs.IR" /></entry>
</feed>`;
const WIKIPEDIA = JSON.stringify({ type: "standard", title: "Cloudflare", displaytitle: "<b>Cloudflare</b>", extract: "Reference result.", content_urls: { desktop: { page: "https://en.wikipedia.org/wiki/Cloudflare" } } });
const EMPTY_WIKIPEDIA = JSON.stringify({});
const DUCKDUCKGO = `<!doctype html><html><body><div id="links"><div class="web-result"><h2><a href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fcloudflare">Example result</a></h2><a class="result__snippet">Web result.</a></div></div></body></html>`;
const BRAVE = `<!doctype html><html><body><div class="snippet" data-type="web"><a class="l1" href="https://example.com/brave"><div class="title search-snippet-title">Brave result</div></a><div class="content">Brave web result.</div></div></body></html>`;
const QWANT = JSON.stringify({ status: "success", data: { result: { items: { mainline: [{ type: "web", items: [{ title: "Qwant result", url: "https://example.com/qwant", desc: "Qwant web result.", source: "example.com" }] }] } } } });
const PUBMED_SEARCH = JSON.stringify({ esearchresult: { idlist: ["12345678"] } });
const PUBMED_SUMMARY = JSON.stringify({ result: { uids: ["12345678"], "12345678": { title: "Food safety and Worker systems", sortpubdate: "2026/06/15 00:00", source: "J Edge Med", fulljournalname: "Journal of Edge Medicine", authors: [{ name: "Ada Example" }], pubtype: ["Journal Article"], articleids: [{ idtype: "doi", value: "10.1234/pubmed.example" }] } } });
const SEMANTIC_HOME = '<!doctype html><html><head><meta name="s2-ui-version" content="test-ui-version"></head></html>';
const SEMANTIC_SCHOLAR = JSON.stringify({ results: [{ id: "abcdef123456", title: { text: "Semantic Scholar result" }, paperAbstract: { text: "Academic web result." }, pubDate: "2026-07-02", authors: [[{ name: "Grace Researcher" }]] }] });
const CROSSREF = JSON.stringify({ status: "ok", message: { items: [{ DOI: "10.1234/crossref.example", URL: "https://doi.org/10.1234/crossref.example", title: ["Crossref result"], abstract: "<jats:p>Crossref academic result.</jats:p>", author: [{ given: "Katherine", family: "Example" }], published: { "date-parts": [[2026, 7, 1]] }, "container-title": ["Journal of Edge Research"] }] } });
const GITHUB = JSON.stringify({ total_count: 1, incomplete_results: false, items: [{ id: 123456, full_name: "cloudflare/workers-rs", html_url: "https://github.com/cloudflare/workers-rs", description: "Write Cloudflare Workers in Rust via WebAssembly.", language: "Rust", stargazers_count: 5000, forks_count: 400, open_issues_count: 80, topics: ["cloudflare-workers", "rust", "wasm"], updated_at: "2026-07-20T12:00:00Z", clone_url: "https://github.com/cloudflare/workers-rs.git", default_branch: "main", owner: { login: "cloudflare", avatar_url: "https://avatars.githubusercontent.com/u/314135" }, license: { spdx_id: "Apache-2.0" } }] });
const MOJEEK = `<!doctype html><html><body><ul class="results-standard"><li><h2><a href="https://example.com/mojeek">Mojeek result</a></h2><a class="ob" href="https://example.com/mojeek">example.com/mojeek</a><p class="s">Independent web search result.</p></li></ul></body></html>`;
const YAHOO = `<!doctype html><html><body><div class="algo-sr"><div class="compTitle"><a href="https://r.search.yahoo.com/_ylt=x/RU=https%3A%2F%2Fexample.com%2Fyahoo/RK=2/RS=x"><h3><span>Yahoo result</span></h3></a></div><div class="compText">Yahoo web search result.</div></div></body></html>`;
const YANDEX = `<!doctype html><html><body><li class="serp-item"><h2><a href="https://example.com/yandex">Yandex result</a></h2><div class="text-container">Yandex web result.</div></li></body></html>`;
const BAIDU = `<!doctype html><html><body><div class="result c-container"><h3><a href="https://www.baidu.com/link?url=example">Baidu result</a></h3><div class="c-abstract">Baidu web result.</div></div></body></html>`;
const GOOGLE = `<!doctype html><html><body><div class="MjjYud"><a href="https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fgoogle&sa=U"><h3>Google result</h3></a><div class="VwiC3b">Google web result.</div></div></body></html>`;
const GROKIPEDIA = JSON.stringify({ results: [{ id: "gp-1", title: "Cloudflare", slug: "cloudflare", snippet: "Reference result." }] });
const OPENALEX = JSON.stringify({ results: [{ id: "https://openalex.org/W1", doi: "https://doi.org/10.1/example", title: "OpenAlex result", publication_date: "2026-07-01", primary_location: {}, authorships: [], cited_by_count: 3, open_access: { is_oa: true } }] });
const STARTPAGE_HOME = '<!doctype html><html><form id="search"><input name="sc" value="test-sc"></form></html>';
const STARTPAGE = '<!doctype html><script>React.createElement(UIStartpage.AppSerpWeb, {"render":{"presenter":{"regions":{"mainline":[{"display_type":"web-google","results":[{"title":"Startpage result","clickUrl":"https://example.com/startpage","description":"Private web result."}]}]}}}});</script>';

function mockProviders({ duckFailure = false, delayArxiv = false, emptyWikipedia = false } = {}) {
  const semanticScholarBodies = [];
  const mock = vi.fn(async (input, init) => {
    const request = input instanceof Request ? input : new Request(input, init);
    const url = new URL(request.url);
    if (url.hostname === "export.arxiv.org") {
      if (delayArxiv) return await new Promise((_, reject) => request.signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError"))));
      return new Response(ARXIV, { status: 200, headers: { "content-type": "application/atom+xml" } });
    }
    if (url.hostname.endsWith("wikipedia.org")) return new Response(emptyWikipedia ? EMPTY_WIKIPEDIA : WIKIPEDIA, { status: 200, headers: { "content-type": "application/json; charset=UTF-8" } });
    if (url.hostname === "html.duckduckgo.com") {
      if (duckFailure) return new Response("<html><form id='challenge-form'>CAPTCHA</form></html>", { status: 403, headers: { "content-type": "text/html" } });
      return new Response(DUCKDUCKGO, { status: 200, headers: { "content-type": "text/html" } });
    }
    if (url.hostname === "search.brave.com") return new Response(BRAVE, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    if (url.hostname === "api.qwant.com") return new Response(QWANT, { status: 200, headers: { "content-type": "application/json" } });
    if (url.hostname === "eutils.ncbi.nlm.nih.gov") return new Response(url.pathname.endsWith("/esearch.fcgi") ? PUBMED_SEARCH : PUBMED_SUMMARY, { status: 200, headers: { "content-type": "application/json" } });
    if (url.hostname === "www.semanticscholar.org") {
      if (url.pathname === "/api/1/search") semanticScholarBodies.push(await request.clone().json());
      return new Response(
        url.pathname === "/" ? SEMANTIC_HOME : SEMANTIC_SCHOLAR,
        { status: 200, headers: { "content-type": url.pathname === "/" ? "text/html" : "application/json" } },
      );
    }
    if (url.hostname === "api.crossref.org") return new Response(CROSSREF, { status: 200, headers: { "content-type": "application/json" } });
    if (url.hostname === "api.github.com") return new Response(GITHUB, { status: 200, headers: { "content-type": "application/json; charset=utf-8" } });
    if (url.hostname === "www.mojeek.com") return new Response(MOJEEK, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    if (url.hostname === "search.yahoo.com" || url.hostname.endsWith(".search.yahoo.com")) return new Response(YAHOO, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    if (url.hostname === "yandex.com" || url.hostname === "www.yandex.com") return new Response(YANDEX, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    if (url.hostname === "www.baidu.com") return new Response(BAIDU, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    if (url.hostname === "www.google.com") return new Response(GOOGLE, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    if (url.hostname === "grokipedia.com") return new Response(GROKIPEDIA, { status: 200, headers: { "content-type": "application/json" } });
    if (url.hostname === "api.openalex.org") return new Response(OPENALEX, { status: 200, headers: { "content-type": "application/json" } });
    if (url.hostname === "mcp.exa.ai") {
      const body = JSON.parse(await request.text());
      if (body.method === "initialize") return new Response(`event: message\ndata: ${JSON.stringify({ jsonrpc: "2.0", id: 1, result: { protocolVersion: "2025-06-18" } })}\n\n`, { status: 200, headers: { "content-type": "text/event-stream", "mcp-session-id": "test-session" } });
      if (body.method === "notifications/initialized") return new Response("", { status: 202 });
      return new Response(`event: message\ndata: ${JSON.stringify({ jsonrpc: "2.0", id: 2, result: { content: [{ type: "text", text: JSON.stringify({ results: [{ title: "Exa result", url: "https://example.com/exa", summary: "Hosted MCP result." }] }) }] } })}\n\n`, { status: 200, headers: { "content-type": "text/event-stream" } });
    }
    if (url.hostname === "www.startpage.com") return new Response(
      url.pathname === "/" ? STARTPAGE_HOME : STARTPAGE,
      { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } },
    );
    if (url.hostname === "example.com") return new Response(
      "<!doctype html><html><head><title>Crawled result</title></head><body><main>Cloudflare crawl index content.</main><a href=\"https://example.com/discovered\">Next page</a></body></html>",
      { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } },
    );
    throw new Error(`unexpected outbound request: ${request.url}`);
  });
  mock.semanticScholarBodies = semanticScholarBodies;
  vi.stubGlobal("fetch", mock);
  return mock;
}

afterEach(() => vi.unstubAllGlobals());

describe("Worker routes", () => {
  it("serves the public search interface", async () => {
    const response = await exports.default.fetch("https://example.com/");
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("text/html");
    const html = await response.text();
    expect(html).toContain("Searxflare");
    expect(html).toContain("/assets/searxflarelogo.svg");
    expect(html).toContain("@paper-design/shaders@0.0.77");
    expect(html).toContain("const fragmentShader=");
    expect(html).not.toContain("shaders.ditheringFragmentShader");
    expect(html).toContain("transform:translateX(-.35%)");
    expect(html).toContain('class="results-shell"');
    expect(html).toContain("activateResults()");
    expect(html).toContain('encodeURIComponent(query)+"&limit=20"');
  });

  it("serves the supplied Searxflare logo with preserved SVG scaling", async () => {
    const response = await exports.default.fetch("https://example.com/assets/searxflarelogo.svg");
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("image/svg+xml");
    const svg = await response.text();
    expect(svg).toContain('viewBox="0 0 522 149"');
    expect(svg).toContain("knowledge is power.");
  });

  it("lets the rate-limited interface search without exposing the API key", async () => {
    const mock = mockProviders();
    const response = await exports.default.fetch("https://example.com/ui/search?q=cloudflare");
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.results.length).toBeGreaterThan(0);
    expect(body.resolved_engines).toHaveLength(19);
    expect(mock.semanticScholarBodies).toContainEqual(expect.objectContaining({ pageSize: 20 }));
  });

  it("rejects REST result limits below ten", async () => {
    const response = await exports.default.fetch(new Request(
      "https://example.com/v1/search?q=cloudflare&limit=9",
      { headers: AUTH },
    ));
    expect(response.status).toBe(400);
    const body = await response.json();
    expect(body.code).toBe("INVALID_REQUEST");
    expect(body.errors).toContainEqual({
      field: "limit",
      message: "limit must be between 10 and 20",
    });
  });

  it("serves unauthenticated health checks", async () => {
    const response = await exports.default.fetch("https://example.com/healthz");
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ status: "ok" });
    expect(response.headers.get("x-request-id")).toBeTruthy();
  });

  it("publishes Web Bot Auth identity and signs opted-in requests", async () => {
    const directory = await exports.default.fetch("https://example.com/.well-known/http-message-signatures-directory");
    expect(directory.status).toBe(200);
    expect(directory.headers.get("content-type")).toContain("application/http-message-signatures-directory+json");
    expect((await directory.json()).keys[0].kid).toBe("test");

    const mock = mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=bot-auth-signature&engines=brave-web", { headers: AUTH }));
    expect(response.status).toBe(200);
    const request = mock.mock.calls
      .map(([input, init]) => input instanceof Request ? input : new Request(input, init))
      .find((candidate) => new URL(candidate.url).hostname === "search.brave.com");
    expect(request.headers.get("signature-agent")).toBe("\"https://example.com/.well-known/http-message-signatures-directory\"");
    expect(request.headers.get("signature-input")).toContain("keyid=\"https://example.com/.well-known/http-message-signatures-directory#test\"");
    expect(request.headers.get("signature-input")).toContain("tag=\"web-bot-auth\"");
    expect(request.headers.get("signature")).toMatch(/^sig1=:/);
  });

  it("protects the v1 API", async () => {
    const response = await exports.default.fetch("https://example.com/v1/engines");
    expect(response.status).toBe(401);
    const problem = await response.json();
    expect(problem.code).toBe("AUTHENTICATION_REQUIRED");
    expect(response.headers.get("content-type")).toContain("application/problem+json");
  });

  it("returns the compile-time catalogue", async () => {
    const response = await exports.default.fetch(new Request("https://example.com/v1/engines", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.engines.map((engine) => engine.id)).toEqual([
      "arxiv", "wikipedia", "duckduckgo-html", "brave-web", "brave-news", "qwant-web", "pubmed", "semantic-scholar", "crossref", "github", "mojeek-web", "yahoo-web", "yandex-web", "baidu-web", "google-web", "grokipedia", "openalex", "exa-mcp", "startpage-web"
    ]);
    expect(body.engines.every((engine) => engine.default_enabled)).toBe(true);
  });

  it("returns UNKNOWN_ENGINE for unsupported IDs", async () => {
    const response = await exports.default.fetch(new Request("https://example.com/v1/engines/nope", { headers: AUTH }));
    expect(response.status).toBe(404);
    expect((await response.json()).code).toBe("UNKNOWN_ENGINE");
  });

  it("fans out concurrently and returns partial results", async () => {
    mockProviders({ duckFailure: true });
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=cloudflare+rust", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(true);
    expect(body.ranking).toBe("query-aware-v1");
    expect(body.results.length).toBeGreaterThanOrEqual(3);
    expect(body.engines.some((engine) => engine.failure_kind === "ENGINE_CHALLENGED")).toBe(true);
    expect(body.engines.map((engine) => engine.engine_id)).toContain("qwant-web");
    expect(body.results.every((result) => Object.keys(result.provider_metadata).length >= 1)).toBe(true);
  });

  it("merges AI Search chunks, deduplicates URLs, and publishes final score order", () => {
    const provider = {
      url: "https://example.com/workers",
      canonical_url: "https://example.com/workers",
      title: "Unrelated provider result",
      content: "",
      engines: ["brave-web"],
      positions: { "brave-web": 1 },
      provider_metadata: { "brave-web": {} },
      metadata: {},
      score: 99,
    };
    const duplicate = aiChunkToResult({
      score: 0.8,
      text: "# Cloudflare Workers Rust\nRelevant indexed content.",
      item: { key: "documents/one.md", metadata: { url: provider.url } },
      scoring_details: { fusion_method: "rrf" },
    }, 1);
    const indexed = aiChunkToResult({
      score: 0.7,
      text: "# Cloudflare Workers Rust Guide\nCloudflare Workers Rust reference.",
      item: { key: "documents/two.md", metadata: { url: "https://example.com/rust-guide" } },
      scoring_details: { fusion_method: "rrf" },
    }, 2);

    const merged = mergeRankedResults(
      [provider],
      [duplicate, indexed],
      "cloudflare workers rust",
      "query-aware-v1",
      10,
    );
    expect(merged).toHaveLength(2);
    expect(merged[0].url).toBe("https://example.com/rust-guide");
    expect(merged[1].engines).toEqual(["brave-web", "ai-search-crawl"]);
    expect(merged[0].score).toBeGreaterThanOrEqual(merged[1].score);
  });

  it("resolves category bangs before provider execution", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=!web+cloudflare+rust", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.query).toBe("!web cloudflare rust");
    expect(body.provider_query).toBe("cloudflare rust");
    expect(body.bangs).toEqual(["web"]);
    expect(body.resolved_categories).toEqual(["general"]);
    expect(body.resolved_engines).toEqual(expect.arrayContaining(["duckduckgo-html", "brave-web"]));
    expect(body.resolved_engines).toContain("qwant-web");
  });

  it("supports the GitHub bang alias", async () => {
    const mock = mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=!gh+cloudflare+workers+rust", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.resolved_engines).toEqual(["github"]);
    expect(body.results[0].title).toBe("cloudflare/workers-rs");
    expect(body.results[0].provider_metadata.github.stars).toBe(5000);
    const githubRequest = mock.mock.calls.map(([input, init]) => input instanceof Request ? input : new Request(input, init)).find((request) => new URL(request.url).hostname === "api.github.com");
    expect(new URL(githubRequest.url).searchParams.get("q")).toBe("cloudflare workers rust");
  });

  it("rejects conflicting, unknown and bang-only queries", async () => {
    for (const query of ["!web !arxiv cloudflare", "!unknown cloudflare", "!web"]) {
      const response = await exports.default.fetch(new Request(`https://example.com/v1/search?q=${encodeURIComponent(query)}`, { headers: AUTH }));
      expect(response.status).toBe(400);
      expect((await response.json()).code).toBe("INVALID_REQUEST");
    }
  });

  it("preserves escaped exclamation marks as literal query text", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request(`https://example.com/v1/search?q=${encodeURIComponent("\\!gh cloudflare")}&engines=wikipedia`, { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.bangs).toEqual([]);
    expect(body.provider_query).toBe("!gh cloudflare");
    expect(body.resolved_engines).toEqual(["wikipedia"]);
  });

  it("aggregates the first provider tranche", async () => {
    const mock = mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=cloudflare&page=2&limit=10&engines=mojeek-web,yahoo-web,qwant-web", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(false);
    expect(body.results.flatMap((result) => result.engines)).toEqual(expect.arrayContaining(["mojeek-web", "yahoo-web", "qwant-web"]));
    const requests = mock.mock.calls.map(([input, init]) => input instanceof Request ? input : new Request(input, init));
    expect(new URL(requests.find((request) => new URL(request.url).hostname === "www.mojeek.com").url).searchParams.get("s")).toBe("10");
    expect(new URL(requests.find((request) => new URL(request.url).hostname.endsWith("search.yahoo.com")).url).searchParams.get("b")).toBe("15");
    const qwant = new URL(requests.find((request) => new URL(request.url).hostname === "api.qwant.com").url);
    expect(qwant.searchParams.get("count")).toBe("10");
    expect(qwant.searchParams.get("offset")).toBe("10");
  });

  it("aggregates the remaining public providers", async () => {
    const mock = mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=cloudflare&page=2&limit=10&engines=yandex-web,baidu-web,google-web,grokipedia", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(false);
    expect(body.results.flatMap((result) => result.engines)).toEqual(expect.arrayContaining(["yandex-web", "baidu-web", "google-web", "grokipedia"]));
    const requests = mock.mock.calls.map(([input, init]) => input instanceof Request ? input : new Request(input, init));
    expect(new URL(requests.find((request) => new URL(request.url).hostname === "yandex.com").url).searchParams.get("p")).toBe("1");
    const baidu = new URL(requests.find((request) => new URL(request.url).hostname === "www.baidu.com").url);
    expect(baidu.searchParams.get("pn")).toBe("10");
    expect(baidu.searchParams.get("rn")).toBe("10");
    const google = new URL(requests.find((request) => new URL(request.url).hostname === "www.google.com").url);
    expect(google.searchParams.get("start")).toBe("10");
    expect(google.searchParams.get("num")).toBe("10");
    const grokipedia = new URL(requests.find((request) => new URL(request.url).hostname === "grokipedia.com").url);
    expect(grokipedia.searchParams.get("limit")).toBe("10");
    expect(grokipedia.searchParams.get("offset")).toBe("10");
  });

  it("aggregates explicit academic providers", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=food+safety&engines=pubmed,semantic-scholar,crossref", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(false);
    expect(body.results.flatMap((result) => result.engines)).toEqual(expect.arrayContaining(["pubmed", "semantic-scholar", "crossref"]));
  });

  it("uses OpenAlex and hosted Exa MCP without API keys", async () => {
    const mock = mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=cloudflare&engines=openalex,exa-mcp", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(false);
    expect(body.results.flatMap((result) => result.engines)).toEqual(expect.arrayContaining(["openalex", "exa-mcp"]));
    const requests = mock.mock.calls.map(([input, init]) => input instanceof Request ? input : new Request(input, init));
    const openalex = requests.find((request) => new URL(request.url).hostname === "api.openalex.org");
    expect(new URL(openalex.url).searchParams.has("api_key")).toBe(false);
    expect(requests.filter((request) => new URL(request.url).hostname === "mcp.exa.ai").every((request) => !request.headers.has("x-api-key"))).toBe(true);
  });

  it("keeps an empty successful engine as a partial response during shared cooldown", async () => {
    mockProviders({ duckFailure: true, emptyWikipedia: true });
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=missing&engines=wikipedia,duckduckgo-html", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(true);
    expect(body.results).toEqual([]);
    expect(body.engines.find((engine) => engine.engine_id === "wikipedia").failure_kind).toBeUndefined();
    expect(body.engines.find((engine) => engine.engine_id === "duckduckgo-html").failure_kind).toBe("ENGINE_RATE_LIMITED");
  });

  it("returns NO_ENGINE_SUCCEEDED when the only engine times out", async () => {
    mockProviders({ delayArxiv: true });
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=rust&engines=arxiv&timeout_ms=10", { headers: AUTH }));
    expect(response.status).toBe(502);
    expect((await response.json()).code).toBe("NO_ENGINE_SUCCEEDED");
  });

  it("serves the SearXNG JSON compatibility subset with bangs", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request("https://example.com/search?q=!wp+rust&format=json", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.query).toBe("!wp rust");
    expect(body.number_of_results).toBe(1);
    expect(body.results[0].engines).toContain("wikipedia");
  });

  it("reuses the aggregate Cache API entry", async () => {
    const mock = mockProviders();
    const request = () => new Request("https://example.com/v1/search?q=cache-test&engines=wikipedia", { headers: AUTH });
    const firstCtx = createExecutionContext();
    const firstWorker = new WorkerEntrypoint(firstCtx, env);
    const first = await firstWorker.fetch(request());
    expect(first.status).toBe(200);
    expect((await first.json()).cached).toBe(false);
    await waitOnExecutionContext(firstCtx);
    const secondCtx = createExecutionContext();
    const secondWorker = new WorkerEntrypoint(secondCtx, env);
    const second = await secondWorker.fetch(request());
    expect(second.status).toBe(200);
    expect((await second.json()).cached).toBe(true);
    const wikipediaSearchCalls = mock.mock.calls
      .map(([input, init]) => input instanceof Request ? input : new Request(input, init))
      .filter((request) => new URL(request.url).pathname.startsWith("/api/rest_v1/page/summary/"));
    expect(wikipediaSearchCalls).toHaveLength(1);
    await waitOnExecutionContext(secondCtx);
  });

  it("crawls successful results after the response and records index state", async () => {
    mockProviders();
    const ctx = createExecutionContext();
    const worker = new WorkerEntrypoint(ctx, env);
    const response = await worker.fetch(new Request(
      "https://example.com/v1/search?q=crawl-index&engines=qwant-web",
      { headers: AUTH },
    ));
    expect(response.status).toBe(200);
    await waitOnExecutionContext(ctx);

    const documents = await env.CRAWL_DOCUMENTS.list({ prefix: "documents/" });
    expect(documents.objects.length).toBeGreaterThan(0);
    const stored = await env.CRAWL_DOCUMENTS.get(documents.objects[0].key);
    expect(await stored.text()).toContain("Cloudflare crawl index content.");

    const frontier = await env.CRAWL_STATE.list({ prefix: "frontier:" });
    expect(frontier.keys.length).toBeGreaterThan(0);
    const scheduledKey = frontier.keys[0].name;
    await worker.scheduled();
    expect(await env.CRAWL_STATE.get(scheduledKey)).toBeNull();
  });
});
