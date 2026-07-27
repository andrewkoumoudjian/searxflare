import RustWorker, { ProviderCoordinatorObject } from "../build/index.js";
import { ditherTransitionFragmentShader } from "./dither-transition-shader.mjs";

export { ProviderCoordinatorObject };

const MAX_CRAWL_BODY_BYTES = 1024 * 1024;
const MAX_CRAWL_TEXT_CHARS = 256 * 1024;
const DEFAULT_CRAWL_PAGES_PER_QUERY = 3;
const DEFAULT_AI_RESULTS = 5;
const DEFAULT_API_RESULTS = 10;
const UI_RESULTS = 20;

const LOGO_SVG = `<?xml version="1.0" encoding="UTF-8"?>
<svg width="522px" height="149px" viewBox="0 0 522 149" version="1.1" xmlns="http://www.w3.org/2000/svg">
  <title>SearxFlare — knowledge is power.</title>
  <g stroke="none" stroke-width="1" fill="none" fill-rule="evenodd">
    <text font-family="Gambarino-Regular, Gambarino" font-size="120" font-weight="normal" fill="#000000">
      <tspan x="0" y="96">SearxFlare</tspan>
    </text>
    <text font-family="SFPro-Regular, SF Pro" font-size="48" font-weight="normal" fill="#000000">
      <tspan x="55" y="137">knowledge is power.</tspan>
    </text>
  </g>
</svg>`;

const HOME_HTML = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <link rel="icon" href="/assets/searxflarelogo.svg?v=2" type="image/svg+xml">
  <title>Searxflare</title>
  <style>
    :root{
      color-scheme:light;
      --paper-rgb:246 246 243;
      --paper:rgb(var(--paper-rgb));
      --ink:#171816;
      --ink-muted:rgb(23 24 22 / 62%);
      --ink-faint:rgb(23 24 22 / 13%);
      --surface:rgb(255 255 255 / 76%);
      --blue:#1a0dab;
      --blue-hover:#174ea6;
      --ease-out:cubic-bezier(.23,1,.32,1);
      font-family:-apple-system,BlinkMacSystemFont,"SF Pro Text","SF Pro Display","Helvetica Neue",Helvetica,Arial,sans-serif;
      color:var(--ink);
      background:var(--paper);
      font-synthesis:none;
      text-rendering:optimizeLegibility;
      -webkit-font-smoothing:antialiased;
    }
    *{box-sizing:border-box}
    html{min-height:100%;overflow-x:hidden;overflow-x:clip;background:var(--paper)}
    body{min-height:100vh;min-height:100svh;margin:0;overflow-x:hidden;background:var(--paper)}
    button,input{font:inherit}
    button{-webkit-tap-highlight-color:transparent}
    .dither-backdrop{position:fixed;z-index:0;inset:0;overflow:hidden;opacity:.62;pointer-events:none;transition:opacity 360ms var(--ease-out)}
    .dither-backdrop canvas{width:100%!important;height:100%!important;opacity:0;animation:shader-enter 700ms var(--ease-out) forwards}
    @keyframes shader-enter{to{opacity:1}}
    body.has-results .dither-backdrop{opacity:.08}
    .page{position:relative;z-index:1;min-height:100vh;min-height:100svh}
    .search-shell{width:min(720px,calc(100% - 32px));margin:0 auto;padding-top:clamp(7.5rem,22vh,13rem);transition:width 360ms var(--ease-out),margin 360ms var(--ease-out),padding 360ms var(--ease-out)}
    .search-layout{position:relative;isolation:isolate}
    .search-layout::before{position:absolute;z-index:-1;inset:-4rem -6rem;background:rgb(var(--paper-rgb) / 28%);content:"";mask-image:radial-gradient(ellipse 100% 74% at center,rgb(0 0 0 / 42%) 0%,rgb(0 0 0 / 26%) 46%,transparent 100%);pointer-events:none}
    .brand-link{display:block;width:clamp(17rem,42vw,28rem);margin:0 auto 2.4rem;color:inherit}
    .brand{display:block;width:100%;height:auto;clip-path:inset(1px);mix-blend-mode:multiply;transform:translateX(-.35%)}
    .search-form{display:flex;min-height:48px;border:1px solid rgb(23 24 22 / 14%);border-radius:24px;align-items:center;padding:3px 7px 3px 17px;background:var(--surface);box-shadow:0 1px 2px rgb(23 24 22 / 5%),0 4px 18px rgb(23 24 22 / 8%);backdrop-filter:blur(16px);transition:border-color 180ms ease,box-shadow 180ms ease,background 180ms ease}
    .search-form:focus-within,.search-form:hover{border-color:rgb(23 24 22 / 20%);background:rgb(255 255 255 / 88%);box-shadow:0 1px 2px rgb(23 24 22 / 6%),0 6px 22px rgb(23 24 22 / 11%)}
    .search-input{min-width:0;border:0;outline:0;flex:1;padding:9px 4px;background:transparent;color:var(--ink);font-size:1rem;letter-spacing:-.01em;line-height:1.45}
    .search-input::-webkit-search-cancel-button{display:none}
    .search-actions{display:flex;align-items:center}
    .icon-button{display:inline-grid;width:40px;height:40px;border:0;border-radius:50%;place-items:center;background:transparent;color:rgb(23 24 22 / 58%);cursor:pointer;transition:background 150ms ease,color 150ms ease}
    .icon-button:hover{background:rgb(23 24 22 / 6%);color:var(--ink)}
    .icon-button:focus-visible,.result-link:focus-visible,.brand-link:focus-visible{outline:2px solid #1a73e8;outline-offset:3px}
    .clear-button[hidden]{display:none}
    .submit-button{color:var(--ink)}
    .icon{width:20px;height:20px;fill:none;stroke:currentColor;stroke-linecap:round;stroke-linejoin:round;stroke-width:1.8}
    .results-shell{width:min(680px,calc(100% - 32px));margin:0 auto;padding:0 0 5rem;opacity:0;transform:translateY(12px);transition:opacity 240ms ease 80ms,transform 300ms var(--ease-out)}
    .status{min-height:22px;margin:1.25rem 0 1.5rem;color:var(--ink-muted);font-size:.875rem;line-height:1.55}
    .results{list-style:none;margin:0;padding:0}
    .result{margin:0 0 2rem}
    .result-source{display:grid;grid-template-columns:28px minmax(0,1fr);grid-template-rows:auto auto;column-gap:.7rem;margin-bottom:.3rem;align-items:center}
    .result-favicon{grid-row:1/3;display:grid;width:28px;height:28px;border:1px solid var(--ink-faint);border-radius:50%;place-items:center;background:rgb(255 255 255 / 70%);color:var(--ink);font-size:.78rem;font-weight:600;text-transform:uppercase}
    .result-site{overflow:hidden;color:var(--ink);font-size:.875rem;line-height:1.3;text-overflow:ellipsis;white-space:nowrap}
    .result-url{overflow:hidden;color:var(--ink-muted);font-size:.75rem;line-height:1.35;text-overflow:ellipsis;white-space:nowrap}
    .result-link{display:inline-block;color:var(--blue);font-family:"SF Pro Display",-apple-system,BlinkMacSystemFont,"Helvetica Neue",sans-serif;font-size:1.25rem;font-weight:400;letter-spacing:-.018em;line-height:1.3;text-decoration:none}
    .result-link:hover{color:var(--blue-hover);text-decoration:underline;text-underline-offset:.13em}
    .result-snippet{display:-webkit-box;max-width:640px;margin:.32rem 0 0;overflow:hidden;color:#3c4043;font-size:.875rem;line-height:1.58;-webkit-box-orient:vertical;-webkit-line-clamp:4}
    .result-engines{margin-top:.35rem;color:var(--ink-muted);font-size:.72rem;line-height:1.4}
    .empty-state{padding:1.5rem 0;color:var(--ink-muted);line-height:1.6}
    body.has-results .search-shell{width:auto;margin:0;padding:1rem 1.5rem .9rem;border-bottom:1px solid var(--ink-faint);background:rgb(var(--paper-rgb) / 90%);backdrop-filter:blur(14px)}
    body.has-results .search-layout{display:grid;grid-template-columns:112px minmax(280px,680px) 1fr;gap:1.5rem;align-items:center}
    body.has-results .brand-link{width:112px;margin:0}
    body.has-results .results-shell{margin-left:calc(112px + 3rem);opacity:1;transform:none}
    @media(max-width:760px){
      .search-shell{padding-top:clamp(5.25rem,16vh,8rem)}
      .brand-link{width:min(20rem,82vw)}
      body.has-results .search-shell{padding:.75rem 1rem}
      body.has-results .search-layout{grid-template-columns:1fr;gap:.65rem}
      body.has-results .brand-link{width:106px;margin-left:.3rem}
      body.has-results .results-shell{margin:0 auto}
      .status{margin-top:1rem}
    }
    @media(max-width:420px){
      .search-shell,.results-shell{width:calc(100% - 24px)}
      .search-form{padding-left:13px}
      .result-snippet{-webkit-line-clamp:5}
    }
    @media(prefers-reduced-motion:reduce){
      *,*::before,*::after{scroll-behavior:auto!important;animation-duration:.01ms!important;animation-iteration-count:1!important;transition-duration:.01ms!important}
    }
  </style>
</head>
<body>
<div id="dither" class="dither-backdrop" aria-hidden="true"></div>
<main class="page">
  <header class="search-shell">
    <div class="search-layout">
      <a class="brand-link" href="/" aria-label="Searxflare home">
        <img class="brand" src="/assets/searxflarelogo.svg?v=2" width="522" height="149" alt="SearxFlare — knowledge is power.">
      </a>
      <form id="search" class="search-form" role="search">
        <input id="q" class="search-input" name="q" type="search" autocomplete="off" autofocus required aria-label="Search">
        <div class="search-actions">
          <button id="clear" class="icon-button clear-button" type="button" aria-label="Clear search" hidden>
            <svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17"/></svg>
          </button>
          <button class="icon-button submit-button" type="submit" aria-label="Search">
            <svg class="icon" viewBox="0 0 24 24" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.4"/><path d="m16 16 4 4"/></svg>
          </button>
        </div>
      </form>
    </div>
  </header>
  <section class="results-shell" aria-label="Search results">
    <div id="status" class="status" role="status" aria-live="polite"></div>
    <ol id="results" class="results"></ol>
  </section>
</main>
<script>
const form=document.querySelector("#search"),input=document.querySelector("#q"),clear=document.querySelector("#clear"),status=document.querySelector("#status"),list=document.querySelector("#results");
function addText(parent,tag,text,className){const node=document.createElement(tag);if(className)node.className=className;node.textContent=text;parent.append(node);return node}
function activateResults(){document.body.classList.add("has-results")}
function updateClear(){clear.hidden=!input.value}
function urlDetails(raw){
  try{
    const parsed=new URL(raw);
    const host=parsed.hostname.replace(/^www\\./,"");
    const parts=parsed.pathname.split("/").filter(Boolean).slice(0,3).map(part=>{try{return decodeURIComponent(part).replace(/[-_]/g," ")}catch{return part}});
    return {host:host,label:host.split(".")[0],trail:host+(parts.length?" › "+parts.join(" › "):"")};
  }catch{return {host:raw,label:"W",trail:raw}}
}
async function search(query){
  activateResults();status.textContent="Searching…";list.replaceChildren();
  const response=await fetch("/ui/search?q="+encodeURIComponent(query)+"&limit=20");
  const body=await response.json();
  if(!response.ok)throw new Error(body.detail||body.message||"Search failed");
  status.textContent=body.result_count+" result"+(body.result_count===1?"":"s")+(body.partial?" · some engines did not respond":"");
  if(!body.results.length){addText(list,"li","No results matched your search. Try different keywords.","empty-state");return}
  for(const result of body.results){
    const item=document.createElement("li");item.className="result";
    const details=urlDetails(result.canonical_url||result.url);
    const source=document.createElement("div");source.className="result-source";
    addText(source,"span",(details.label[0]||"W").toUpperCase(),"result-favicon");
    addText(source,"span",details.host,"result-site");
    addText(source,"span",details.trail,"result-url");
    item.append(source);
    const link=addText(item,"a",result.title||result.url,"result-link");link.href=result.url;link.rel="noopener noreferrer";
    if(result.content)addText(item,"p",result.content,"result-snippet");
    if(result.engines?.length)addText(item,"div",result.engines.join(" · "),"result-engines");
    list.append(item);
  }
}
form.addEventListener("submit",event=>{event.preventDefault();const query=input.value.trim();if(!query)return;activateResults();history.pushState({query:query},"","/?q="+encodeURIComponent(query));search(query).catch(error=>status.textContent=error.message)});
input.addEventListener("input",updateClear);
clear.addEventListener("click",()=>{input.value="";updateClear();input.focus()});
window.addEventListener("popstate",()=>{const query=new URL(location.href).searchParams.get("q");if(query){input.value=query;updateClear();search(query).catch(error=>status.textContent=error.message)}else{document.body.classList.remove("has-results");input.value="";updateClear();status.textContent="";list.replaceChildren()}});
const initial=new URL(location.href).searchParams.get("q");if(initial){input.value=initial;updateClear();activateResults();search(initial).catch(error=>status.textContent=error.message)}
async function initShader(){
  if(window.matchMedia("(prefers-reduced-motion: reduce)").matches||window.matchMedia("(prefers-reduced-data: reduce)").matches)return;
  try{
    const shaders=await import("https://esm.sh/@paper-design/shaders@0.0.77");
    const fragmentShader=${JSON.stringify(ditherTransitionFragmentShader)};
    const color=value=>shaders.getShaderColorFromString(value);
    const presets=[
      {color:"#87909a",mask:[.64,.2,.92],shape:shaders.DitheringShapes.simplex,size:1.65,scale:.92,speed:.46,rotation:-10,offset:[.14,-.24]},
      {color:"#059669",mask:[.72,.26,.9],shape:shaders.DitheringShapes.ripple,size:1.7,scale:.9,speed:.2,rotation:0,offset:[.3,-.24]},
      {color:"#6fae45",mask:[.18,.46,.88],shape:shaders.DitheringShapes.dots,size:2.2,scale:1.2,speed:.32,rotation:12,offset:[-.42,0]},
      {color:"#4267bd",mask:[.76,.64,.96],shape:shaders.DitheringShapes.sphere,size:1.9,scale:.94,speed:.63,rotation:0,offset:[.38,.28]},
      {color:"#92724c",mask:[.32,.7,1.12],shape:shaders.DitheringShapes.wave,size:1.75,scale:.84,speed:.82,rotation:-8,offset:[-.3,.36]}
    ];
    const mount=new shaders.ShaderMount(document.querySelector("#dither"),fragmentShader,{
      u_colorBack:shaders.getShaderColorFromString("#f6f6f3"),
      u_transitionStyle:1,
      u_transitionProgress:1,
      u_targetIndex:0,
      u_transitionOrigin:[.64,.2],
      "u_colorFronts[0]":presets.map(preset=>color(preset.color)),
      "u_masks[0]":presets.map(preset=>preset.mask),
      "u_motion[0]":presets.map(preset=>[preset.rotation,preset.speed]),
      "u_offsets[0]":presets.map(preset=>preset.offset),
      "u_params[0]":presets.map((preset,index)=>[preset.shape,preset.size,preset.scale,index===4?3:0]),
      u_weights:[1,0,0,0],
      u_dktWeight:0
    },undefined,1,0,1,window.innerWidth<=700?400000:800000);
    window.addEventListener("pagehide",()=>mount.dispose(),{once:true});
  }catch(error){console.warn("Shader fallback active",error)}
}
void initShader();
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
      "content-security-policy": "default-src 'self'; img-src 'self'; script-src 'unsafe-inline' https://esm.sh; style-src 'unsafe-inline'; connect-src 'self' https://esm.sh; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
    },
  });
}

function logoResponse() {
  return new Response(LOGO_SVG, {
    headers: {
      "content-type": "image/svg+xml; charset=utf-8",
      "cache-control": "public, max-age=86400",
      "x-content-type-options": "nosniff",
    },
  });
}

function boundedInteger(value, fallback, minimum, maximum) {
  const parsed = Number.parseInt(value ?? "", 10);
  return Number.isFinite(parsed) ? Math.min(maximum, Math.max(minimum, parsed)) : fallback;
}

async function searchContextFromRequest(request) {
  const url = new URL(request.url);
  let query = (url.searchParams.get("q") || url.searchParams.get("query") || "").trim();
  let limit = url.searchParams.get("limit");
  if (request.method === "POST" && url.pathname === "/v1/search") {
    try {
      const body = await request.clone().json();
      query = String(body.q || body.query || query).trim();
      limit = body.limit ?? limit;
    } catch {
      // The Rust route returns the canonical invalid-body problem response.
    }
  }
  return {
    query,
    limit: url.pathname === "/ui/search"
      ? UI_RESULTS
      : boundedInteger(limit, DEFAULT_API_RESULTS, DEFAULT_API_RESULTS, UI_RESULTS),
  };
}

function isEnrichableSearchPath(pathname) {
  return pathname === "/v1/search" || pathname === "/ui/search";
}

function isCrawlableSearchPath(pathname) {
  return isEnrichableSearchPath(pathname)
    || pathname === "/search"
    || /^\/v1\/engines\/[^/]+\/search$/.test(pathname);
}

function safeCrawlUrl(raw) {
  try {
    const url = new URL(raw);
    if (url.protocol !== "https:" || url.username || url.password) return null;
    const host = url.hostname.toLowerCase();
    if (host === "localhost" || host.endsWith(".localhost") || host.endsWith(".internal")) return null;
    if (/^(0|10|127|169\.254|172\.(1[6-9]|2\d|3[01])|192\.168)\./.test(host)) return null;
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
  const safeTitle = page.title.replace(/[\r\n]+/g, " ").trim();
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
  ].join("\n");
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
      if (text.length < MAX_CRAWL_TEXT_CHARS) text += chunk.text.replace(/\s+/g, " ") + " ";
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
    title: title.replace(/\s+/g, " ").trim(),
    text: text.replace(/\s+/g, " ").trim().slice(0, MAX_CRAWL_TEXT_CHARS),
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
  const normalized = value => String(value || "").toLocaleLowerCase().normalize("NFKC").replace(/[^\p{L}\p{N}]+/gu, " ").trim();
  const needle = normalized(query);
  const terms = [...new Set(needle.split(/\s+/).filter(Boolean))];
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

function mergeRankedResults(providerResults, aiResults, query, ranking, limit) {
  const merged = new Map();
  const add = (result, rank, source) => {
    const key = result.canonical_url || result.url;
    if (!key) return;
    const contribution = 1 / (60 + rank);
    if (!merged.has(key)) {
      merged.set(key, {
        result: structuredClone(result),
        providerRank: source === "provider" ? rank : Number.POSITIVE_INFINITY,
        fusionScore: contribution,
      });
      return;
    }
    const entry = merged.get(key);
    entry.fusionScore += contribution;
    if (source === "provider") entry.providerRank = Math.min(entry.providerRank, rank);
    entry.result.engines = [...new Set([...(entry.result.engines || []), ...(result.engines || [])])];
    entry.result.positions = { ...(entry.result.positions || {}), ...(result.positions || {}) };
    entry.result.provider_metadata = {
      ...(entry.result.provider_metadata || {}),
      ...(result.provider_metadata || {}),
    };
    entry.result.metadata = { ...(entry.result.metadata || {}), ...(result.metadata || {}) };
  };
  providerResults.forEach((result, index) => add(result, index + 1, "provider"));
  aiResults.forEach((result, index) => add(result, index + 1, "index"));

  const entries = [...merged.values()];
  entries.sort((left, right) => {
    if (ranking === "query-aware-v1") {
      const relevance = lexicalScore(right.result, query) - lexicalScore(left.result, query);
      if (relevance) return relevance;
    }
    const fusion = right.fusionScore - left.fusionScore;
    if (fusion) return fusion;
    return left.providerRank - right.providerRank;
  });
  return entries.slice(0, limit).map(entry => entry.result);
}

function aiChunkToResult(chunk, position) {
  const metadata = chunk.item?.metadata || {};
  const text = String(chunk.text || "");
  const frontmatterUrl = text.match(/^url:\s*["']?([^"'\n]+)["']?/m)?.[1];
  const url = safeCrawlUrl(metadata.url || frontmatterUrl);
  if (!url) return null;
  const heading = text.match(/^#\s+(.+)$/m)?.[1]?.trim();
  return {
    url: url.href,
    canonical_url: url.href,
    title: metadata.title || heading || url.hostname,
    content: text.replace(/^---[\s\S]*?---\s*/m, "").replace(/^#\s+.*$/m, "").trim().slice(0, 600),
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

async function enrichSearchResponse(response, searchContext, env, ctx, enrich) {
  if (!response.ok || !response.headers.get("content-type")?.includes("application/json")) return response;
  const body = await response.clone().json();
  if (!Array.isArray(body.results)) return response;
  ctx.waitUntil(crawlResults(body.results, env));
  if (!enrich) return response;
  try {
    const aiResults = await aiSearchResults(searchContext.query, env);
    body.results = mergeRankedResults(
      body.results,
      aiResults,
      searchContext.query,
      body.ranking,
      searchContext.limit,
    );
    body.result_count = body.results.length;
    if (aiResults.length) {
      body.resolved_engines = [...new Set([...(body.resolved_engines || []), "ai-search-crawl"])];
      body.engines = [...(body.engines || []), {
        engine_id: "ai-search-crawl", duration_ms: 0, cache_status: "bypass",
        result_count: aiResults.length, response_bytes: 0, parse_ms: 0, redirect_count: 0,
        parser_version: "cloudflare-ai-search-hybrid-rrf-v1",
      }];
    }
    const headers = new Headers(response.headers);
    headers.delete("content-length");
    return new Response(JSON.stringify(body), { status: response.status, headers });
  } catch (error) {
    console.warn(JSON.stringify({ event: "ai_search_failed", message: String(error) }));
    return response;
  }
}

export default class extends RustWorker {
  async fetch(request) {
    const url = new URL(request.url);
    if (request.method === "GET" && url.pathname === "/") return htmlResponse();
    if (request.method === "GET" && url.pathname === "/assets/searxflarelogo.svg") return logoResponse();
    const searchContext = isCrawlableSearchPath(url.pathname)
      ? await searchContextFromRequest(request)
      : null;
    const response = await super.fetch(request);
    return searchContext
      ? enrichSearchResponse(
          response,
          searchContext,
          this.env,
          this.ctx,
          isEnrichableSearchPath(url.pathname),
        )
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
