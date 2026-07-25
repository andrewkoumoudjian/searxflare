# Ranking

## `rrf-v1`

```text
sum(engine_weight / (60 + provider_position))
+ 0.10 * ln(1 + contributing_engine_count)
```

## `searx-compat-v1`

Compatibility follows SearXNG's documented result-container formula: multiply contributing engine weights, multiply by the number of contributing positions, then sum that value divided by each provider position.

Both strategies reject zero positions and invalid weights, sort descending by score and use canonical URL ascending as the final stable tie-breaker.
