use crate::{arxiv::ArxivEngine, duckduckgo_html::DuckDuckGoHtmlEngine, wikipedia::WikipediaEngine};
use metasearch_core::{EngineContext, EngineDescriptor, EngineFailure, EngineOutput, NormalizedQuery, SearchEngine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisteredEngine {
    Arxiv,
    Wikipedia,
    DuckDuckGoHtml,
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for RegisteredEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        match self {
            Self::Arxiv => ArxivEngine.descriptor(),
            Self::Wikipedia => WikipediaEngine.descriptor(),
            Self::DuckDuckGoHtml => DuckDuckGoHtmlEngine.descriptor(),
        }
    }

    async fn search(
        &self,
        query: &NormalizedQuery,
        context: &EngineContext<'_>,
    ) -> Result<EngineOutput, EngineFailure> {
        match self {
            Self::Arxiv => ArxivEngine.search(query, context).await,
            Self::Wikipedia => WikipediaEngine.search(query, context).await,
            Self::DuckDuckGoHtml => DuckDuckGoHtmlEngine.search(query, context).await,
        }
    }
}

static REGISTRY: [RegisteredEngine; 3] = [
    RegisteredEngine::Arxiv,
    RegisteredEngine::Wikipedia,
    RegisteredEngine::DuckDuckGoHtml,
];

pub fn registry() -> &'static [RegisteredEngine] {
    &REGISTRY
}

pub fn find_engine(engine_id: &str) -> Option<&'static RegisteredEngine> {
    REGISTRY.iter().find(|engine| engine.descriptor().id == engine_id)
}

pub fn default_engine_ids() -> Vec<String> {
    REGISTRY
        .iter()
        .filter(|engine| engine.descriptor().default_enabled)
        .map(|engine| engine.descriptor().id.to_owned())
        .collect()
}
