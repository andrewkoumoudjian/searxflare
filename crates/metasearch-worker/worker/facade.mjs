import RustWorker, { ProviderCoordinatorObject } from "../build/index.js";

export { ProviderCoordinatorObject };

const MAX_CRAWL_BODY_BYTES = 1024 * 1024;
const MAX_CRAWL_TEXT_CHARS = 256 * 1024;
const DEFAULT_CRAWL_PAGES_PER_QUERY = 3;
const DEFAULT_AI_RESULTS = 5;

const HOME_HTML = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Searxflare</title>
  <style>
    :root{color-scheme:light;font-family:Arial,Helvetica,sans-serif;color:#202124}
    *{box-sizing:border-box}body{margin:0;background:#fff}
    main{width:min(720px,calc(100% - 32px));margin:0 auto;padding:18vh 0 72px}
    h1{text-align:center;font-size:clamp(2.8rem,9vw,5.2rem);font-weight:500;letter-spacing:-.08em;margin:0 0 34px}
    h1 span:nth-child(1),h1 span:nth-child(4){color:#4285f4}h1 span:nth-child(2),h1 span:nth-child(6){color:#ea4335}
    h1 span:nth-child(3){color:#fbbc05}h1 span:nth-child(5){color:#34a853}
    form{display:flex;align-items:center;border:1px solid #dfe1e5;border-radius:24px;padding:4px 6px 4px 18px;box-shadow:0 1px 6px rgba(32,33,36,.12)}
    form:focus-within{box-shadow:0 1px 8px rgba(32,33,36,.24)}
    input{border:0;outline:0;flex:1;font-size:16px;padding:10px 4px;background:transparent}
    button{border:0;border-radius:20px;background:#1a73e8;color:#fff;font-weight:600;padding:10px 18px;cursor:pointer}
    #status{min-height:22px;margin:24px 4px 8px;color:#5f6368}
    ol{list-style:none;padding:0;margin:0}.result{margin:0 0 28px}
    .url{color:#202124;font-size:14px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
    a{font-size:20px;color:#1a0dab;text-decoration:none}a:hover{text-decoration:underline}
    p{margin:6px 0 0;line-height:1.55;color:#4d5156}.engines{font-size:12px;color:#70757a;margin-top:5px}
    @media(max-width:520px){main{padding-top:11vh}h1{margin-bottom:26px}button{padding-inline:13px}}
  </style>
</head>
<body>
<main>
  <h1 aria-label="Searxflare"><span>S</span><span>e</span><span>a</span><span>r</span><span>x</span><span>flare</span></h1>
  <form id="search"><input id="q" name="q" type="search" autocomplete="off" autofocus required aria-label="Search"><button>Search</button></form>
  <div id="status" role="status"></div><ol id="results"></ol>
</main>
<script>
const form=document.querySelector("#search"),input=document.querySelector("#q"),status=document.querySelector("#status"),list=document.querySelector("#results");
function addText(parent,tag,text,className){const node=document.createElement(tag);if(className)node.className=className;node.textContent=text;parent.append(node);return node}
async function search(query){
  status.textContent="Searching…";list.replaceChildren();
  const response=await fetch("/ui/search?q="+encodeURIComponent(query));
  const body=await response.json();
  if(!response.ok)throw new Error(body.detail||body.message||"Search failed");
  status.textContent=body.result_count+" result"+(body.result_count===1?"":"s")+(body.partial?" · some engines did not respond":"");
  for(const result of body.results){
    const item=document.createElement("li");item.className="result";
    addText(item,"div",result.canonical_url||result.url,"url");
    const link=addText(item,"a",result.title||result.url);link.href=result.url;link.rel="noopener noreferrer";
    if(result.content)addText(item,"p",result.content);
    if(result.engines?.length)addText(item,"div",result.engines.join(" · "),"engines");
    list.append(item);
  }
}
form.addEventListener("submit",event=>{event.preventDefault();const query=input.value.trim();if(!query)return;history.replaceState(null,"","/?q="+encodeURIComponent(query));search(query).catch(error=>status.textContent=error.message)});
const initial=new URL(location.href).searchParams.get("q");if(initial){input.value=initial;search(initial).catch(error=>status.textContent=error.message)}
</script>
</body>
</html>`;

function htmlResponse() {
  return new Response(HOME_HTML, {
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "public, max-age=300",
      "x-content-type-options": "nosniff",
      "referrer-policy": "no-referrer",
      "content-security-policy": "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
    },
  });
}

function boundedInteger(value, fallback, minimum, maximum) {
  const parsed = Number.parseInt(value ?? "", 10);
  return Number.isFinite(parsed) ? Math.min(maximum, Math.max(minimum, parsed)) : fallback;
}

function queryFromRequest(request) {
  const url = new URL(request.url);
  return (url.searchParams.get("q") || url.searchParams.get("query") || "").trim();
}

function isSearchPath(pathname) {
  return pathname === "/v1/search" || pathname === "/ui/search";
}

function safeCrawlUrl(raw) {
  try {
    const url = new URL(raw);
    if (url.protocol !== "https:" || url.username || url.password) return null;
    const host = url.hostname.toLowerCase();
    if (host === "localhost" || host.endsWith(".localhost") || host.endsWith(".internal")) return null;
    if (/^(0|10|127|169\\.254|172\\.(1[6-9]|2\\d|3[01])|192\\.168)\\./.test(host)) return null;
    url.hash = "";
    return url;
  } catch {
    return null;
  }
}

async function sha256Hex(value) {
  const bytes = new TextEncoder().encode(value);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, "0")).join("");
}

function markdownDocument(page) {
  const safeTitle = page.title.replace(/[\\r\\n]+/g, " ").trim();
  return [
    "---",
    `url: ${JSON.stringify(page.url)}`,
    `title: ${JSON.stringify(safeTitle)}`,
    `fetched_at: ${JSON.stringify(page.fetchedAt)}`,
    "---",
    "",
    `# ${safeTitle || page.url}`,
    "",
    page.text,
    "",
    "## Discovered links",
    "",
    ...page.links.map(link => `- ${link}`),
    "",
  ].join("\\n");
}

async function crawlPage(rawUrl, env) {
  const url = safeCrawlUrl(rawUrl);
  if (!url || !env.CRAWL_DOCUMENTS) return;
  const digest = await sha256Hex(url.href);
  const marker = `page:${digest}`;
  if (env.CRAWL_STATE && await env.CRAWL_STATE.get(marker)) return;

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 8000);
  let response;
  try {
    response = await fetch(url, {
      headers: { accept: "text/html,application/xhtml+xml;q=0.9", "user-agent": "SearxflareCrawler/1.0" },
      redirect: "follow",
      signal: controller.signal,
    });
  } finally {
    clearTimeout(timeout);
  }
  if (!response.ok) return;
  const contentType = response.headers.get("content-type") || "";
  if (!contentType.toLowerCase().includes("text/html")) return;
  const declaredLength = Number(response.headers.get("content-length") || "0");
  if (declaredLength > MAX_CRAWL_BODY_BYTES) return;

  let title = "";
  let text = "";
  const links = new Set();
  const finalUrl = safeCrawlUrl(response.url) || url;
  const rewriter = new HTMLRewriter()
    .on("title", { text(chunk) { if (title.length < 512) title += chunk.text; } })
    .on("body", { text(chunk) {
      if (text.length < MAX_CRAWL_TEXT_CHARS) text += chunk.text.replace(/\\s+/g, " ") + " ";
    }})
    .on("a[href]", { element(element) {
      if (links.size >= 64) return;
      const href = element.getAttribute("href");
      if (!href) return;
      const next = safeCrawlUrl(new URL(href, finalUrl).href);
      if (next) links.add(next.href);
    }});
  const transformed = rewriter.transform(response);
  const bytes = await transformed.arrayBuffer();
  if (bytes.byteLength > MAX_CRAWL_BODY_BYTES) return;

  const page = {
    url: finalUrl.href,
    title: title.replace(/\\s+/g, " ").trim(),
    text: text.replace(/\\s+/g, " ").trim().slice(0, MAX_CRAWL_TEXT_CHARS),
    links: [...links],
    fetchedAt: new Date().toISOString(),
  };
  if (!page.text) return;
  await env.CRAWL_DOCUMENTS.put(`documents/${digest}.md`, markdownDocument(page), {
    httpMetadata: { contentType: "text/markdown; charset=utf-8" },
    customMetadata: { url: page.url, title: page.title.slice(0, 1024) },
  });
  if (env.CRAWL_STATE) {
    await env.CRAWL_STATE.put(marker, page.fetchedAt, { expirationTtl: 86400 });
    await env.CRAWL_STATE.put(`links:${digest}`, JSON.stringify(page.links), { expirationTtl: 604800 });
    for (const link of page.links.slice(0, 12)) {
      const linkDigest = await sha256Hex(link);
      await env.CRAWL_STATE.put(`frontier:${linkDigest}`, link, { expirationTtl: 604800 });
    }
  }
}

async function crawlResults(results, env) {
  const limit = boundedInteger(env.CRAWL_MAX_PAGES_PER_QUERY, DEFAULT_CRAWL_PAGES_PER_QUERY, 0, 10);
  const urls = [...new Set((results || []).map(result => result.url).filter(Boolean))].slice(0, limit);
  await Promise.allSettled(urls.map(url => crawlPage(url, env)));
}

function lexicalScore(result, query) {
  const normalized = value => String(value || "").toLocaleLowerCase().normalize("NFKC").replace(/[^\\p{L}\\p{N}]+/gu, " ").trim();
  const needle = normalized(query);
  const terms = [...new Set(needle.split(/\\s+/).filter(Boolean))];
  const title = normalized(result.title);
  const content = normalized(result.content);
  const url = normalized(result.canonical_url || result.url);
  const coverage = value => terms.length ? terms.filter(term => value.split(" ").some(token => token === term || (term.length >= 4 && token.startsWith(term)))).length / terms.length : 0;
  return (needle && title.includes(needle) ? 2 : 0)
    + (needle && content.includes(needle) ? 0.75 : 0)
    + 1.5 * coverage(title)
    + 0.6 * coverage(content)
    + 0.25 * coverage(url)
    + 0.1 * Math.log1p(result.engines?.length || 1)
    + 0.05 * Number(result.metadata?.ai_search_score || 0);
}

function aiChunkToResult(chunk, position) {
  const metadata = chunk.item?.metadata || {};
  const text = String(chunk.text || "");
  const frontmatterUrl = text.match(/^url:\\s*["']?([^"'\\n]+)["']?/m)?.[1];
  const url = safeCrawlUrl(metadata.url || frontmatterUrl);
  if (!url) return null;
  const heading = text.match(/^#\\s+(.+)$/m)?.[1]?.trim();
  return {
    url: url.href,
    canonical_url: url.href,
    title: metadata.title || heading || url.hostname,
    content: text.replace(/^---[\\s\\S]*?---\\s*/m, "").replace(/^#\\s+.*$/m, "").trim().slice(0, 600),
    category: "general",
    metadata: { ai_search_score: chunk.score, ai_search_item: chunk.item?.key },
    provider_metadata: { "ai-search-crawl": { score: chunk.score, fusion_method: chunk.scoring_details?.fusion_method || "rrf" } },
    engines: ["ai-search-crawl"],
    positions: { "ai-search-crawl": position },
    score: Number(chunk.score || 0),
  };
}

async function aiSearchResults(query, env) {
  if (!query || !env.CRAWL_SEARCH || String(env.ENABLE_AI_SEARCH).toLowerCase() !== "true") return [];
  const maxResults = boundedInteger(env.AI_SEARCH_MAX_RESULTS, DEFAULT_AI_RESULTS, 1, 10);
  const response = await env.CRAWL_SEARCH.search({
    query,
    ai_search_options: {
      retrieval: {
        retrieval_type: "hybrid",
        fusion_method: "rrf",
        max_num_results: maxResults,
        return_on_failure: true,
      },
      query_rewrite: { enabled: false },
      reranking: { enabled: false },
      cache: { enabled: true },
    },
  });
  return (response.chunks || []).map((chunk, index) => aiChunkToResult(chunk, index + 1)).filter(Boolean);
}

async function enrichSearchResponse(response, query, env, ctx) {
  if (!response.ok || !response.headers.get("content-type")?.includes("application/json")) return response;
  const body = await response.clone().json();
  if (!Array.isArray(body.results)) return response;
  ctx.waitUntil(crawlResults(body.results, env));
  try {
    const aiResults = await aiSearchResults(query, env);
    const merged = new Map();
    for (const result of [...body.results, ...aiResults]) {
      const key = result.canonical_url || result.url;
      if (!merged.has(key)) merged.set(key, result);
      else {
        const existing = merged.get(key);
        existing.engines = [...new Set([...(existing.engines || []), ...(result.engines || [])])];
      }
    }
    body.results = [...merged.values()].sort((left, right) => lexicalScore(right, query) - lexicalScore(left, query)).slice(0, 20);
    body.result_count = body.results.length;
    if (aiResults.length) {
      body.resolved_engines = [...new Set([...(body.resolved_engines || []), "ai-search-crawl"])];
      body.engines = [...(body.engines || []), {
        engine_id: "ai-search-crawl", duration_ms: 0, cache_status: "bypass",
        result_count: aiResults.length, response_bytes: 0, parse_ms: 0, redirect_count: 0,
        parser_version: "cloudflare-ai-search-hybrid-rrf-v1",
      }];
    }
    return new Response(JSON.stringify(body), { status: response.status, headers: response.headers });
  } catch (error) {
    console.warn(JSON.stringify({ event: "ai_search_failed", message: String(error) }));
    return response;
  }
}

export default class extends RustWorker {
  async fetch(request) {
    const url = new URL(request.url);
    if (request.method === "GET" && url.pathname === "/") return htmlResponse();
    const query = queryFromRequest(request);
    const response = await super.fetch(request);
    return isSearchPath(url.pathname)
      ? enrichSearchResponse(response, query, this.env, this.ctx)
      : response;
  }

  async scheduled() {
    if (!this.env.CRAWL_STATE || !this.env.CRAWL_DOCUMENTS) return;
    const batch = await this.env.CRAWL_STATE.list({ prefix: "frontier:", limit: 4 });
    for (const key of batch.keys) {
      const url = await this.env.CRAWL_STATE.get(key.name);
      if (url) await crawlPage(url, this.env);
      await this.env.CRAWL_STATE.delete(key.name);
    }
  }
}
