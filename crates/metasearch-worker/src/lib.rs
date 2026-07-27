#[cfg(target_arch = "wasm32")]
mod runtime;

#[cfg(target_arch = "wasm32")]
use metasearch_core::{ProviderCoordinatorCommand, ProviderCoordinatorSnapshot};
#[cfg(target_arch = "wasm32")]
use worker::*;

#[cfg(target_arch = "wasm32")]
#[event(fetch)]
pub async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    runtime::handle(req, env, ctx).await
}

#[cfg(target_arch = "wasm32")]
#[durable_object]
pub struct ProviderCoordinatorObject {
    state: State,
}

#[cfg(target_arch = "wasm32")]
impl DurableObject for ProviderCoordinatorObject {
    fn new(state: State, _env: Env) -> Self {
        Self { state }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        if req.method() != Method::Post || req.path() != "/command" {
            return Response::error("not found", 404);
        }
        let command = req.json::<ProviderCoordinatorCommand>().await?;
        let now_ms = match command {
            ProviderCoordinatorCommand::AcquireRefreshLease { now_ms, .. }
            | ProviderCoordinatorCommand::RecordSuccess { now_ms }
            | ProviderCoordinatorCommand::RecordFailure { now_ms, .. }
            | ProviderCoordinatorCommand::Snapshot { now_ms } => now_ms,
        };
        let storage = self.state.storage();
        let mut snapshot = storage
            .get::<ProviderCoordinatorSnapshot>("snapshot")
            .await?
            .unwrap_or_default();
        if snapshot
            .lease_expires_at_ms
            .is_some_and(|expiry| expiry <= now_ms)
        {
            snapshot.lease_expires_at_ms = None;
        }
        if snapshot
            .cooldown_expires_at_ms
            .is_some_and(|expiry| expiry <= now_ms)
        {
            snapshot.cooldown_expires_at_ms = None;
            snapshot.consecutive_failures = 0;
            snapshot.last_failure_kind = None;
        }
        snapshot.refresh_lease_acquired = false;
        match command {
            ProviderCoordinatorCommand::AcquireRefreshLease { now_ms, lease_ms } => {
                if snapshot.lease_expires_at_ms.is_none()
                    && snapshot.cooldown_expires_at_ms.is_none()
                {
                    snapshot.refresh_lease_acquired = true;
                    snapshot.lease_expires_at_ms = Some(now_ms.saturating_add(u64::from(lease_ms)));
                }
            }
            ProviderCoordinatorCommand::RecordSuccess { .. } => {
                snapshot.lease_expires_at_ms = None;
                snapshot.cooldown_expires_at_ms = None;
                snapshot.consecutive_failures = 0;
                snapshot.last_failure_kind = None;
            }
            ProviderCoordinatorCommand::RecordFailure {
                now_ms,
                failure_kind,
                cooldown_ms,
            } => {
                snapshot.lease_expires_at_ms = None;
                snapshot.cooldown_expires_at_ms =
                    Some(now_ms.saturating_add(u64::from(cooldown_ms)));
                snapshot.consecutive_failures = snapshot.consecutive_failures.saturating_add(1);
                snapshot.last_failure_kind = Some(failure_kind);
            }
            ProviderCoordinatorCommand::Snapshot { .. } => {}
        }
        storage.put("snapshot", &snapshot).await?;
        Response::from_json(&snapshot)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_test_marker() -> &'static str {
    "metasearch-worker compiles natively for tests; production runs on wasm32-unknown-unknown"
}
