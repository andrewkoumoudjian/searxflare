import { env, exports } from "cloudflare:workers";
import { createExecutionContext, waitOnExecutionContext } from "cloudflare:test";
import { afterEach, describe, expect, it, vi } from "vitest";
import WorkerEntrypoint from "../../crates/metasearch-worker/build/worker/shim.mjs";

const AUTH = { authorization: "Bearer test-api-key" };

const ARXIV = `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <entry>
    <id>https://arxiv.org/abs/1234.5678v1</id>
    <title>Cloudflare Rust</title>
    <summary>Academic result.</summary>
    <published>2026-01-02T03:04:05Z</published>
    <author><name>Ada Example</name></author>
    <link title="pdf" href="https://arxiv.org/pdf/1234.5678v1" type="application/pdf" />
    <category term="cs.IR" />
  </entry>
</feed>`;

const WIKIPEDIA = JSON.stringify({
  batchcomplete: true,
  query: {
    pages: [
      {
        pageid: 42,
        ns: 0,
        title: "Cloudflare",
        extract: "Reference result.",
        fullurl: "https://en.wikipedia.org/wiki/Cloudflare"
      }
    ]
  }
});

const EMPTY_WIKIPEDIA = JSON.stringify({ batchcomplete: true });

const DUCKDUCKGO = `<!doctype html><html><body><div id="links">
<div class="web-result"><h2><a href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fcloudflare">Example result</a></h2><a class="result__snippet">Web result.</a></div>
</div></body></html>`;

const BRAVE = `<!doctype html><html><body>
<div class="snippet"><a href="https://example.com/brave"><div class="title">Brave result</div></a><div class="content">Brave web result.</div></div>
</body></html>`;

const QWANT = JSON.stringify({
  status: "success",
  data: {
    result: {
      items: {
        mainline: [
          {
            type: "web",
            items: [
              {
                title: "Qwant result",
                url: "https://example.com/qwant",
                desc: "Qwant web result.",
                source: "example.com"
              }
            ]
          }
        ]
      }
    }
  }
});

const PUBMED_SEARCH = JSON.stringify({
  esearchresult: { idlist: ["12345678"] }
});

const PUBMED_SUMMARY = JSON.stringify({
  result: {
    uids: ["12345678"],
    "12345678": {
      title: "Food safety and Worker systems",
      sortpubdate: "2026/06/15 00:00",
      source: "J Edge Med",
      fulljournalname: "Journal of Edge Medicine",
      authors: [{ name: "Ada Example" }],
      pubtype: ["Journal Article"],
      articleids: [{ idtype: "doi", value: "10.1234/pubmed.example" }]
    }
  }
});

const SEMANTIC_SCHOLAR = JSON.stringify({
  total: 1,
  offset: 0,
  data: [
    {
      paperId: "abcdef123456",
      url: "https://www.semanticscholar.org/paper/abcdef123456",
      title: "Semantic Scholar result",
      abstract: "Academic graph result.",
      publicationDate: "2026-07-02",
      authors: [{ authorId: "1", name: "Grace Researcher" }],
      externalIds: { DOI: "10.1234/semantic.example" },
      citationCount: 8
    }
  ]
});

const CROSSREF = JSON.stringify({
  status: "ok",
  message: {
    items: [
      {
        DOI: "10.1234/crossref.example",
        URL: "https://doi.org/10.1234/crossref.example",
        title: ["Crossref result"],
        abstract: "<jats:p>Crossref academic result.</jats:p>",
        author: [{ given: "Katherine", family: "Example" }],
        published: { "date-parts": [[2026, 7, 1]] },
        "container-title": ["Journal of Edge Research"]
      }
    ]
  }
});

const GITHUB = JSON.stringify({
  total_count: 1,
  incomplete_results: false,
  items: [
    {
      id: 123456,
      full_name: "cloudflare/workers-rs",
      html_url: "https://github.com/cloudflare/workers-rs",
      description: "Write Cloudflare Workers in Rust via WebAssembly.",
      language: "Rust",
      stargazers_count: 5000,
      forks_count: 400,
      open_issues_count: 80,
      topics: ["cloudflare-workers", "rust", "wasm"],
      updated_at: "2026-07-20T12:00:00Z",
      clone_url: "https://github.com/cloudflare/workers-rs.git",
      default_branch: "main",
      owner: {
        login: "cloudflare",
        avatar_url: "https://avatars.githubusercontent.com/u/314135"
      },
      license: { spdx_id: "Apache-2.0" }
    }
  ]
});

function mockProviders({ duckFailure = false, delayArxiv = false, emptyWikipedia = false } = {}) {
  const mock = vi.fn(async (input, init) => {
    const request = input instanceof Request ? input : new Request(input, init);
    const url = new URL(request.url);
    if (url.hostname === "export.arxiv.org") {
      if (delayArxiv) {
        return await new Promise((_, reject) => {
          request.signal.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
        });
      }
      return new Response(ARXIV, { status: 200, headers: { "content-type": "application/atom+xml" } });
    }
    if (url.hostname.endsWith("wikipedia.org")) {
      return new Response(emptyWikipedia ? EMPTY_WIKIPEDIA : WIKIPEDIA, {
        status: 200,
        headers: { "content-type": "application/json; charset=UTF-8" }
      });
    }
    if (url.hostname === "html.duckduckgo.com") {
      if (duckFailure) {
        return new Response("<html><form id='challenge-form'>CAPTCHA</form></html>", { status: 403, headers: { "content-type": "text/html" } });
      }
      return new Response(DUCKDUCKGO, { status: 200, headers: { "content-type": "text/html" } });
    }
    if (url.hostname === "search.brave.com") {
      return new Response(BRAVE, { status: 200, headers: { "content-type": "text/html; charset=UTF-8" } });
    }
    if (url.hostname === "api.qwant.com") {
      return new Response(QWANT, { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.hostname === "eutils.ncbi.nlm.nih.gov") {
      const body = url.pathname.endsWith("/esearch.fcgi") ? PUBMED_SEARCH : PUBMED_SUMMARY;
      return new Response(body, { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.hostname === "api.semanticscholar.org") {
      return new Response(SEMANTIC_SCHOLAR, { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.hostname === "api.crossref.org") {
      return new Response(CROSSREF, { status: 200, headers: { "content-type": "application/json" } });
    }
    if (url.hostname === "api.github.com") {
      return new Response(GITHUB, { status: 200, headers: { "content-type": "application/json; charset=utf-8" } });
    }
    throw new Error(`unexpected outbound request: ${request.url}`);
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("Worker routes", () => {
  it("serves unauthenticated health checks", async () => {
    const response = await exports.default.fetch("https://example.com/healthz");
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ status: "ok" });
    expect(response.headers.get("x-request-id")).toBeTruthy();
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
      "arxiv",
      "wikipedia",
      "duckduckgo-html",
      "brave-web",
      "qwant-web",
      "pubmed",
      "semantic-scholar",
      "crossref",
      "github"
    ]);
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
    expect(body.engines.map((engine) => engine.engine_id)).not.toContain("qwant-web");
    expect(body.engines.map((engine) => engine.engine_id)).not.toContain("pubmed");
    expect(body.results.every((result) => result.canonical_url.startsWith("http"))).toBe(true);
    expect(body.results.every((result) => Object.keys(result.provider_metadata).length >= 1)).toBe(true);
  });

  it("resolves category bangs before provider execution", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request(
      "https://example.com/v1/search?q=!web+cloudflare+rust",
      { headers: AUTH }
    ));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.query).toBe("!web cloudflare rust");
    expect(body.provider_query).toBe("cloudflare rust");
    expect(body.bangs).toEqual(["web"]);
    expect(body.resolved_categories).toEqual(["general"]);
    expect(body.resolved_engines).toEqual(expect.arrayContaining(["duckduckgo-html", "brave-web"]));
    expect(body.resolved_engines).not.toContain("qwant-web");
  });

  it("supports compatible engine bangs and the GitHub alias", async () => {
    const mock = mockProviders();
    const response = await exports.default.fetch(new Request(
      "https://example.com/v1/search?q=!gh+cloudflare+workers+rust",
      { headers: AUTH }
    ));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.provider_query).toBe("cloudflare workers rust");
    expect(body.bangs).toEqual(["gh"]);
    expect(body.resolved_engines).toEqual(["github"]);
    expect(body.results[0].title).toBe("cloudflare/workers-rs");
    expect(body.results[0].provider_metadata.github.stars).toBe(5000);
    const githubRequest = mock.mock.calls
      .map(([input, init]) => input instanceof Request ? input : new Request(input, init))
      .find((request) => new URL(request.url).hostname === "api.github.com");
    expect(new URL(githubRequest.url).searchParams.get("q")).toBe("cloudflare workers rust");
  });

  it("rejects conflicting, unknown and bang-only queries", async () => {
    for (const query of ["!web !arxiv cloudflare", "!unknown cloudflare", "!web"]) {
      const response = await exports.default.fetch(new Request(
        `https://example.com/v1/search?q=${encodeURIComponent(query)}`,
        { headers: AUTH }
      ));
      expect(response.status).toBe(400);
      expect((await response.json()).code).toBe("INVALID_REQUEST");
    }
  });

  it("preserves escaped exclamation marks as literal query text", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request(
      `https://example.com/v1/search?q=${encodeURIComponent("\\!gh cloudflare")}&engines=wikipedia`,
      { headers: AUTH }
    ));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.bangs).toEqual([]);
    expect(body.provider_query).toBe("!gh cloudflare");
    expect(body.resolved_engines).toEqual(["wikipedia"]);
  });

  it("aggregates the independent general web providers", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request(
      "https://example.com/v1/search?q=cloudflare&engines=duckduckgo-html,brave-web,qwant-web",
      { headers: AUTH }
    ));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(false);
    expect(body.results.flatMap((result) => result.engines)).toEqual(expect.arrayContaining([
      "duckduckgo-html",
      "brave-web",
      "qwant-web"
    ]));
  });

  it("aggregates explicit academic providers", async () => {
    mockProviders();
    const response = await exports.default.fetch(new Request(
      "https://example.com/v1/search?q=food+safety&engines=pubmed,semantic-scholar,crossref",
      { headers: AUTH }
    ));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(false);
    expect(body.results.flatMap((result) => result.engines)).toEqual(expect.arrayContaining([
      "pubmed",
      "semantic-scholar",
      "crossref"
    ]));
  });

  it("keeps an empty successful engine as a partial response", async () => {
    mockProviders({ duckFailure: true, emptyWikipedia: true });
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=missing&engines=wikipedia,duckduckgo-html", { headers: AUTH }));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body.partial).toBe(true);
    expect(body.results).toEqual([]);
    expect(body.engines.find((engine) => engine.engine_id === "wikipedia").failure_kind).toBeUndefined();
    expect(body.engines.find((engine) => engine.engine_id === "duckduckgo-html").failure_kind).toBe("ENGINE_CHALLENGED");
  });

  it("returns NO_ENGINE_SUCCEEDED when the only engine times out", async () => {
    mockProviders({ delayArxiv: true });
    const response = await exports.default.fetch(new Request("https://example.com/v1/search?q=rust&engines=arxiv&timeout_ms=10", { headers: AUTH }));
    expect(response.status).toBe(502);
    expect((await response.json()).code).toBe("NO_ENGINE_SUCCEEDED");
  });

  it("serves the SearXNG JSON compatibility subset with bangs", async () => {
    mockProviders();
    const response = await exports.default.fetch("https://example.com/search?q=!wp+rust&format=json");
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
    expect(mock).toHaveBeenCalledTimes(1);
    await waitOnExecutionContext(secondCtx);
  });
});
