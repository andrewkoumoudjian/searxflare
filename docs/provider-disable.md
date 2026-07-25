# Provider disable procedure

1. Confirm the failure using the single-engine debug route and structured logs without recording raw queries.
2. Classify whether the cause is challenge/rate limit, access denial, invalid content type, body limit, timeout or parse change.
3. Set `default_enabled: false` in the descriptor and corresponding manifest.
4. Increment `ENGINE_REGISTRY_VERSION` so cached plans/results cannot mask the change.
5. Add or update a fixture reproducing the failure and ship through CI.
6. Deploy and verify that default searches return partial results without invoking the disabled provider.
7. Re-enable only after a fixture-backed parser/protocol change and production-egress canary succeeds.

Do not weaken host checks, body limits, authentication or challenge classification to restore a provider.
