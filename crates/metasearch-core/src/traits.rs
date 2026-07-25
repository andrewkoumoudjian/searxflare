use crate::{
    BoundedResponse, Deadline, EngineDescriptor, EngineFailure, EngineOutput, EngineRequest,
    NormalizedQuery,
};

#[async_trait::async_trait(?Send)]
pub trait EngineHttpClient {
    async fn send(
        &self,
        engine: &'static EngineDescriptor,
        request: EngineRequest,
        deadline: Deadline,
    ) -> Result<BoundedResponse, EngineFailure>;
}

#[async_trait::async_trait(?Send)]
pub trait EngineState {
    async fn get(&self, engine_id: &str, key: &str) -> Result<Option<Vec<u8>>, EngineFailure>;

    async fn put(
        &self,
        engine_id: &str,
        key: &str,
        value: &[u8],
        ttl_seconds: Option<u64>,
    ) -> Result<(), EngineFailure>;

    async fn delete(&self, engine_id: &str, key: &str) -> Result<(), EngineFailure>;
}

pub struct EngineContext<'a> {
    pub http: &'a dyn EngineHttpClient,
    pub state: &'a dyn EngineState,
    pub deadline: Deadline,
    pub request_id: &'a str,
}

#[async_trait::async_trait(?Send)]
pub trait SearchEngine {
    fn descriptor(&self) -> &'static EngineDescriptor;

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure>;
}
