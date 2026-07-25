# Deployment

## Configure the secret

```bash
printf '%s' "$SEARXFLARE_API_KEY" | shasum -a 256
npx wrangler secret put API_KEY_SHA256
```

Paste only the lowercase hash, not the raw key. Configure optional Analytics Engine and KV bindings in `wrangler.toml` after creating the resources.

## Validate and deploy

```bash
npm run bundle
npx wrangler deploy
```

The CI dry-run proves the Worker builds and bundles; only an authenticated deployment proves account configuration and production egress. After deployment, verify `/healthz`, `/readyz`, the engine catalogue, each single-engine debug route, a default search and a deliberate partial-provider failure.

Do not put Cloudflare Access in front of the cached search route. Apply WAF, API Shield OpenAPI validation and Workers Rate Limiting at the edge. Import `spec/openapi.yaml` into API Shield in log mode before enabling block mode.

Rollback is a Worker version rollback or deployment of the last known-good commit. No database migration is required for this slice.
