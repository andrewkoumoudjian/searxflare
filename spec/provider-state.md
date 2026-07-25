# Provider state

The current engines are stateless and use `NoopEngineState`. `KvEngineState` supports slow-changing snapshots and expiring non-critical values when an `ENGINE_STATE` binding is configured.

`ProviderCoordinator` defines the contract for a future per-provider Durable Object that can perform single-flight token refresh, exact cooldown mutation and failure counters. Search requests do not pass through a Durable Object by default. A coordinator is introduced only for an engine whose protocol requires exact mutation or query-bound continuation state.
