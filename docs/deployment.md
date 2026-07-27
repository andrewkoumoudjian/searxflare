# Deployment

## Configure the application secret

```bash
printf '%s' "$SEARXFLARE_API_KEY" | shasum -a 256
npx wrangler secret put API_KEY_SHA256
```

Paste only the lowercase hash, not the raw key. Configure optional Analytics Engine and KV bindings in `wrangler.toml` after creating the resources.

Configure the cursor key and any provider credentials through Worker secrets:

```bash
npx wrangler secret put CURSOR_SIGNING_KEY
npx wrangler secret put GITHUB_TOKEN
npx wrangler secret put EXA_API_KEY
```

Web Bot Auth additionally uses `WEB_BOT_AUTH_PRIVATE_KEY`,
`WEB_BOT_AUTH_KEY_ID`, `WEB_BOT_AUTH_DIRECTORY_URL`,
`WEB_BOT_AUTH_PUBLIC_JWKS`, and optionally `WEB_BOT_AUTH_AGENT_CARD`.
Generate the Ed25519 private key offline and store only its base64url-encoded
32-byte seed as a secret. Never put the private key in KV or repository config.

The production configuration binds `ENGINE_STATE`, `CRAWL_STATE`,
`CRAWL_DOCUMENTS`, `CRAWL_SEARCH`, and `SEARCH_ANALYTICS`. The `searxflare`
AI Search instance indexes the R2 crawl corpus and must report a healthy indexing
state before `ENABLE_AI_SEARCH=true` is deployed.

## Cloudflare Workers Builds

The repository is self-contained for Cloudflare's Git integration. `scripts/build_worker.sh` reads the pinned toolchain from `rust-toolchain.toml`, installs Rust through the official rustup installer when the build image does not provide it, adds `wasm32-unknown-unknown`, installs `worker-build` 0.8.5 when absent, validates `Cargo.lock`, and builds `metasearch-worker`.

When Cloudflare injects `WORKERS_CI=1`, the build script delegates to `scripts/cloudflare_validate.sh`. That release gate runs formatting, Clippy, native Rust tests, the direct Wasm workspace build, OpenAPI and engine-manifest validation, workerd integration tests, a recursion-guarded Wrangler dry run, and the bundle-size limit before the preview or production upload continues.

Use these Worker build settings from the repository root:

```text
Production branch: main
Root directory:    /
Build command:     leave empty
Deploy command:    npx wrangler deploy
```

Leaving the separate build command empty avoids compiling twice. Wrangler invokes the `[build]` command in `wrangler.toml` during deployment. If the Cloudflare project already has `npm run build` configured, that command is also supported through the root `package.json`, but the later deploy step invokes the build again.

The Worker name in Cloudflare must be `searxflare`, matching `wrangler.toml`. Enable non-production branch builds only when preview deployments are desired. JavaScript dependencies are pinned by `package-lock.json`; CI and manual deployment use `npm ci`.

Do not add `strip = "symbols"` to the Cargo release profile while using the current `workers-rs`/`wasm-bindgen` toolchain. Symbol stripping removes the externref table required for generated catch wrappers and causes `worker-build` to fail after Rust compilation.

## Validate and deploy manually

```bash
npm ci
npm run bundle
npx wrangler deploy
```

GitHub Actions and Workers Builds both run formatting, Clippy, native Rust tests, the direct Wasm build, schema validation, workerd integration tests and the bundle-size guard. Workers Builds also proves that the exact Git head can be uploaded by the configured Cloudflare deployment path.

## Post-deployment verification

Verify:

1. `GET /healthz` returns `200` without authentication.
2. `GET /readyz` returns `200` only when `API_KEY_SHA256` is configured.
3. The authenticated engine catalogue returns the nineteen compiled engines.
4. Each single-engine debug route returns results or a classified provider failure.
5. A default authenticated search returns a deterministic JSON response.
6. A deliberate provider failure produces a partial result when another engine succeeds.
7. `GET /` returns the search interface and `GET /ui/search?q=cloudflare` returns JSON without disclosing the API key.
8. A successful query creates a document under `documents/` in `CRAWL_DOCUMENTS` and emits no crawl errors in Worker logs.

A dry-run proves that the Worker builds and bundles. Only an authenticated deployment proves account configuration and production egress.

Do not put Cloudflare Access in front of the cached search route. Apply WAF, API Shield OpenAPI validation and Workers Rate Limiting at the edge. Import `spec/openapi.yaml` into API Shield in log mode before enabling block mode.

The first deployment containing `ProviderCoordinatorObject` is an atomic Durable
Object lifecycle migration. Deploy that migration separately; later code-only
versions can use `scripts/deploy_gradual.sh`. See `docs/operations.md` for the
rollout and dashboard contract.
