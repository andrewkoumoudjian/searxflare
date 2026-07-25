# Deployment

## Configure the application secret

```bash
printf '%s' "$SEARXFLARE_API_KEY" | shasum -a 256
npx wrangler secret put API_KEY_SHA256
```

Paste only the lowercase hash, not the raw key. Configure optional Analytics Engine and KV bindings in `wrangler.toml` after creating the resources.

## Cloudflare Workers Builds

The repository is self-contained for Cloudflare's Git integration. `scripts/build_worker.sh` reads the pinned toolchain from `rust-toolchain.toml`, installs Rust through the official rustup installer when the build image does not provide it, adds `wasm32-unknown-unknown`, installs `worker-build` 0.8.5 when absent, validates `Cargo.lock`, and builds `metasearch-worker`.

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

The CI and Cloudflare validation paths additionally run formatting, Clippy, native Rust tests, the direct Wasm build, schema validation, workerd integration tests and the bundle-size guard.

## Post-deployment verification

Verify:

1. `GET /healthz` returns `200` without authentication.
2. `GET /readyz` returns `200` only when `API_KEY_SHA256` is configured.
3. The authenticated engine catalogue returns the three compiled engines.
4. Each single-engine debug route returns results or a classified provider failure.
5. A default authenticated search returns a deterministic JSON response.
6. A deliberate provider failure produces a partial result when another engine succeeds.

A dry-run proves that the Worker builds and bundles. Only an authenticated deployment proves account configuration and production egress.

Do not put Cloudflare Access in front of the cached search route. Apply WAF, API Shield OpenAPI validation and Workers Rate Limiting at the edge. Import `spec/openapi.yaml` into API Shield in log mode before enabling block mode.

Rollback is a Worker version rollback or deployment of the last known-good commit. No database migration is required for this slice.
