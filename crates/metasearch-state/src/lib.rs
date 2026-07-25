#[cfg(target_arch = "wasm32")]
use metasearch_core::FailureKind;
use metasearch_core::{EngineFailure, EngineState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopEngineState;

#[async_trait::async_trait(?Send)]
impl EngineState for NoopEngineState {
    async fn get(&self, _engine_id: &str, _key: &str) -> Result<Option<Vec<u8>>, EngineFailure> {
        Ok(None)
    }

    async fn put(
        &self,
        _engine_id: &str,
        _key: &str,
        _value: &[u8],
        _ttl_seconds: Option<u64>,
    ) -> Result<(), EngineFailure> {
        Ok(())
    }

    async fn delete(&self, _engine_id: &str, _key: &str) -> Result<(), EngineFailure> {
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
pub struct KvEngineState {
    kv: worker::kv::KvStore,
    prefix: String,
}

#[cfg(target_arch = "wasm32")]
impl KvEngineState {
    pub fn new(kv: worker::kv::KvStore, prefix: impl Into<String>) -> Self {
        Self {
            kv,
            prefix: prefix.into(),
        }
    }

    fn key(&self, engine_id: &str, key: &str) -> String {
        format!("{}:{engine_id}:{key}", self.prefix)
    }
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
impl EngineState for KvEngineState {
    async fn get(&self, engine_id: &str, key: &str) -> Result<Option<Vec<u8>>, EngineFailure> {
        self.kv
            .get(&self.key(engine_id, key))
            .bytes()
            .await
            .map_err(|error| {
                EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
            })
    }

    async fn put(
        &self,
        engine_id: &str,
        key: &str,
        value: &[u8],
        ttl_seconds: Option<u64>,
    ) -> Result<(), EngineFailure> {
        let builder = self
            .kv
            .put_bytes(&self.key(engine_id, key), value)
            .map_err(|error| {
                EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
            })?;
        let builder = if let Some(ttl) = ttl_seconds {
            builder.expiration_ttl(ttl)
        } else {
            builder
        };
        builder.execute().await.map_err(|error| {
            EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
        })
    }

    async fn delete(&self, engine_id: &str, key: &str) -> Result<(), EngineFailure> {
        self.kv
            .delete(&self.key(engine_id, key))
            .await
            .map_err(|error| {
                EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinatorCommand {
    AcquireRefreshLease {
        now_ms: u64,
        lease_ms: u32,
    },
    RecordSuccess {
        now_ms: u64,
    },
    RecordFailure {
        now_ms: u64,
        failure_kind: String,
        cooldown_ms: u32,
    },
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CoordinatorSnapshot {
    pub lease_expires_at_ms: Option<u64>,
    pub cooldown_expires_at_ms: Option<u64>,
    pub consecutive_failures: u32,
    pub last_failure_kind: Option<String>,
}

#[async_trait::async_trait(?Send)]
pub trait ProviderCoordinator {
    async fn execute(
        &self,
        engine_id: &str,
        command: CoordinatorCommand,
    ) -> Result<CoordinatorSnapshot, EngineFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_contract_round_trips() {
        let command = CoordinatorCommand::RecordFailure {
            now_ms: 100,
            failure_kind: "ENGINE_CHALLENGED".into(),
            cooldown_ms: 30_000,
        };
        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: CoordinatorCommand = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, command);
    }
}
