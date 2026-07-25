# Deployment

## Configure the secret

```bash
printf '%s' "$SEARXFLARE_API_KEY" | shasum -a 256
npx wrangler secret put API_KEY_SHA256
```

Paste only the lowercase hash, not the raw key. Configure optional Analytics Engine and KV bindings in `wrangler.toml` after creating the resources.

## Cloudflare Workers Builds

The repository is self-contained for Cloudflare's Git integration. `scripts/build_worker.sh` reads the pinned toolchain from `rust-toolchain.toml`, installs Rust through the official rustup installer when the build image does not provide it, adds `wasm32-unknown-unknown`, installs the pinned `worker-build` release when absent, and builds `metasearch-worker`.

Use these Worker build settings from the repository root:

```text
Production branch: main
Build command:     leave empty
Deploy command:    npx wrangler deploy
```

Leaving the separate build command empty avoids compiling twice. Wrangler invokes the `[build]` command in `wrangler.toml` during deployment. If the Cloudflare project already has `npm run build` configured, that command is also supported through the root `package.json`, but the later deploy step will invoke the build again.

The Worker name in Cloudflare must be `searxflare`, matching `wrangler.toml`. Enable non-production branch builds only when preview deployments are desired.

## Validate and deploy manually

```bash
npm run bundle
npx wrangler deploy
```

The CI dry-run proves the Worker builds and bundles; only an authenticated deployment proves account configuration and production egress. After deployment, verify `/healthz`, `/readyz`, the engine catalogue, each single-engine debug route, a default search and a deliberate partial-provider failure.

Do not put Cloudflare Access in front of the cached search route. Apply WAF, API Shield OpenAPI validation and Workers Rate Limiting at the edge. Import `spec/openapi.yaml` into API Shield in log mode before enabling block mode.

Rollback is a Worker version rollback or deployment of the last known-good commit. No database migration is required for this slice.
