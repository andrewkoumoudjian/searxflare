#[cfg(target_arch = "wasm32")]
use metasearch_core::FailureKind;
use metasearch_core::{
    EngineFailure, EngineSecrets, EngineState, ProviderCoordinator, ProviderCoordinatorCommand,
    ProviderCoordinatorSnapshot,
};
#[cfg(target_arch = "wasm32")]
use worker::{Headers, Method, ObjectNamespace, Request, RequestInit};

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

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopEngineSecrets;

impl EngineSecrets for NoopEngineSecrets {
    fn get(&self, _engine_id: &str, _name: &str) -> Option<String> {
        None
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopProviderCoordinator;

#[async_trait::async_trait(?Send)]
impl ProviderCoordinator for NoopProviderCoordinator {
    async fn execute(
        &self,
        _engine_id: &str,
        command: ProviderCoordinatorCommand,
    ) -> Result<ProviderCoordinatorSnapshot, EngineFailure> {
        Ok(ProviderCoordinatorSnapshot {
            refresh_lease_acquired: matches!(
                command,
                ProviderCoordinatorCommand::AcquireRefreshLease { .. }
            ),
            ..ProviderCoordinatorSnapshot::default()
        })
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

#[cfg(target_arch = "wasm32")]
pub struct DurableObjectProviderCoordinator {
    namespace: ObjectNamespace,
}

#[cfg(target_arch = "wasm32")]
impl DurableObjectProviderCoordinator {
    pub const fn new(namespace: ObjectNamespace) -> Self {
        Self { namespace }
    }
}

#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait(?Send)]
impl ProviderCoordinator for DurableObjectProviderCoordinator {
    async fn execute(
        &self,
        engine_id: &str,
        command: ProviderCoordinatorCommand,
    ) -> Result<ProviderCoordinatorSnapshot, EngineFailure> {
        let stub = self.namespace.get_by_name(engine_id).map_err(|error| {
            EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
        })?;
        let body = serde_json::to_string(&command).map_err(|error| {
            EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
        })?;
        let headers = Headers::new();
        headers
            .set("content-type", "application/json")
            .map_err(|error| {
                EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
            })?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(body.into()));
        let request = Request::new_with_init("https://provider-coordinator/command", &init)
            .map_err(|error| {
                EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
            })?;
        let mut response = stub.fetch_with_request(request).await.map_err(|error| {
            EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
        })?;
        if response.status_code() != 200 {
            return Err(EngineFailure::new(
                engine_id,
                FailureKind::Internal,
                format!(
                    "provider coordinator returned HTTP {}",
                    response.status_code()
                ),
            ));
        }
        response
            .json::<ProviderCoordinatorSnapshot>()
            .await
            .map_err(|error| {
                EngineFailure::new(engine_id, FailureKind::Internal, error.to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinator_contract_round_trips() {
        let command = ProviderCoordinatorCommand::RecordFailure {
            now_ms: 100,
            failure_kind: "ENGINE_CHALLENGED".into(),
            cooldown_ms: 30_000,
        };
        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: ProviderCoordinatorCommand = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, command);
    }
}
