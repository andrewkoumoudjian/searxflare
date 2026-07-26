use crate::{
    arxiv::ArxivEngine, brave::BraveEngine, crossref::CrossrefEngine,
    duckduckgo_html::DuckDuckGoHtmlEngine, github::GitHubEngine, mojeek::MojeekEngine,
    pubmed::PubMedEngine, qwant::QwantEngine, semantic_scholar::SemanticScholarEngine,
    wikipedia::WikipediaEngine, yahoo::YahooEngine,
};
use metasearch_core::{
    EngineContext, EngineDescriptor, EngineFailure, EngineOutput, NormalizedQuery, SearchEngine,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisteredEngine {
    Arxiv,
    Wikipedia,
    DuckDuckGoHtml,
    Brave,
    Qwant,
    PubMed,
    SemanticScholar,
    Crossref,
    GitHub,
    Mojeek,
    Yahoo,
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for RegisteredEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        match self {
            Self::Arxiv => ArxivEngine.descriptor(),
            Self::Wikipedia => WikipediaEngine.descriptor(),
            Self::DuckDuckGoHtml => DuckDuckGoHtmlEngine.descriptor(),
            Self::Brave => BraveEngine.descriptor(),
            Self::Qwant => QwantEngine.descriptor(),
            Self::PubMed => PubMedEngine.descriptor(),
            Self::SemanticScholar => SemanticScholarEngine.descriptor(),
            Self::Crossref => CrossrefEngine.descriptor(),
            Self::GitHub => GitHubEngine.descriptor(),
            Self::Mojeek => MojeekEngine.descriptor(),
            Self::Yahoo => YahooEngine.descriptor(),
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
            Self::Brave => BraveEngine.search(query, context).await,
            Self::Qwant => QwantEngine.search(query, context).await,
            Self::PubMed => PubMedEngine.search(query, context).await,
            Self::SemanticScholar => SemanticScholarEngine.search(query, context).await,
            Self::Crossref => CrossrefEngine.search(query, context).await,
            Self::GitHub => GitHubEngine.search(query, context).await,
            Self::Mojeek => MojeekEngine.search(query, context).await,
            Self::Yahoo => YahooEngine.search(query, context).await,
        }
    }
}

static REGISTRY: [RegisteredEngine; 11] = [
    RegisteredEngine::Arxiv,
    RegisteredEngine::Wikipedia,
    RegisteredEngine::DuckDuckGoHtml,
    RegisteredEngine::Brave,
    RegisteredEngine::Qwant,
    RegisteredEngine::PubMed,
    RegisteredEngine::SemanticScholar,
    RegisteredEngine::Crossref,
    RegisteredEngine::GitHub,
    RegisteredEngine::Mojeek,
    RegisteredEngine::Yahoo,
];

pub fn registry() -> &'static [RegisteredEngine] {
    &REGISTRY
}

pub fn find_engine(engine_id: &str) -> Option<&'static RegisteredEngine> {
    REGISTRY
        .iter()
        .find(|engine| engine.descriptor().id == engine_id)
}

pub fn default_engine_ids() -> Vec<String> {
    REGISTRY
        .iter()
        .filter(|engine| engine.descriptor().default_enabled)
        .map(|engine| engine.descriptor().id.to_owned())
        .collect()
}
