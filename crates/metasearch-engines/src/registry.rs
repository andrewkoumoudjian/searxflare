use crate::{
    arxiv::ArxivEngine, baidu::BaiduEngine, brave::BraveEngine, brave_news::BraveNewsEngine,
    crossref::CrossrefEngine, duckduckgo_html::DuckDuckGoHtmlEngine, exa_mcp::ExaMcpEngine,
    github::GitHubEngine, google::GoogleEngine, grokipedia::GrokipediaEngine, mojeek::MojeekEngine,
    openalex::OpenAlexEngine, pubmed::PubMedEngine, qwant::QwantEngine,
    semantic_scholar::SemanticScholarEngine, startpage::StartpageEngine,
    wikipedia::WikipediaEngine, wolframalpha::WolframAlphaEngine, yahoo::YahooEngine,
    yandex::YandexEngine,
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
    BraveNews,
    Qwant,
    PubMed,
    SemanticScholar,
    Crossref,
    GitHub,
    Mojeek,
    Yahoo,
    Yandex,
    Baidu,
    Google,
    Grokipedia,
    WolframAlpha,
    OpenAlex,
    ExaMcp,
    Startpage,
}

#[async_trait::async_trait(?Send)]
impl SearchEngine for RegisteredEngine {
    fn descriptor(&self) -> &'static EngineDescriptor {
        match self {
            Self::Arxiv => ArxivEngine.descriptor(),
            Self::Wikipedia => WikipediaEngine.descriptor(),
            Self::DuckDuckGoHtml => DuckDuckGoHtmlEngine.descriptor(),
            Self::Brave => BraveEngine.descriptor(),
            Self::BraveNews => BraveNewsEngine.descriptor(),
            Self::Qwant => QwantEngine.descriptor(),
            Self::PubMed => PubMedEngine.descriptor(),
            Self::SemanticScholar => SemanticScholarEngine.descriptor(),
            Self::Crossref => CrossrefEngine.descriptor(),
            Self::GitHub => GitHubEngine.descriptor(),
            Self::Mojeek => MojeekEngine.descriptor(),
            Self::Yahoo => YahooEngine.descriptor(),
            Self::Yandex => YandexEngine.descriptor(),
            Self::Baidu => BaiduEngine.descriptor(),
            Self::Google => GoogleEngine.descriptor(),
            Self::Grokipedia => GrokipediaEngine.descriptor(),
            Self::WolframAlpha => WolframAlphaEngine.descriptor(),
            Self::OpenAlex => OpenAlexEngine.descriptor(),
            Self::ExaMcp => ExaMcpEngine.descriptor(),
            Self::Startpage => StartpageEngine.descriptor(),
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
            Self::BraveNews => BraveNewsEngine.search(query, context).await,
            Self::Qwant => QwantEngine.search(query, context).await,
            Self::PubMed => PubMedEngine.search(query, context).await,
            Self::SemanticScholar => SemanticScholarEngine.search(query, context).await,
            Self::Crossref => CrossrefEngine.search(query, context).await,
            Self::GitHub => GitHubEngine.search(query, context).await,
            Self::Mojeek => MojeekEngine.search(query, context).await,
            Self::Yahoo => YahooEngine.search(query, context).await,
            Self::Yandex => YandexEngine.search(query, context).await,
            Self::Baidu => BaiduEngine.search(query, context).await,
            Self::Google => GoogleEngine.search(query, context).await,
            Self::Grokipedia => GrokipediaEngine.search(query, context).await,
            Self::WolframAlpha => WolframAlphaEngine.search(query, context).await,
            Self::OpenAlex => OpenAlexEngine.search(query, context).await,
            Self::ExaMcp => ExaMcpEngine.search(query, context).await,
            Self::Startpage => StartpageEngine.search(query, context).await,
        }
    }
}

static REGISTRY: [RegisteredEngine; 20] = [
    RegisteredEngine::Arxiv,
    RegisteredEngine::Wikipedia,
    RegisteredEngine::DuckDuckGoHtml,
    RegisteredEngine::Brave,
    RegisteredEngine::BraveNews,
    RegisteredEngine::Qwant,
    RegisteredEngine::PubMed,
    RegisteredEngine::SemanticScholar,
    RegisteredEngine::Crossref,
    RegisteredEngine::GitHub,
    RegisteredEngine::Mojeek,
    RegisteredEngine::Yahoo,
    RegisteredEngine::Yandex,
    RegisteredEngine::Baidu,
    RegisteredEngine::Google,
    RegisteredEngine::Grokipedia,
    RegisteredEngine::WolframAlpha,
    RegisteredEngine::OpenAlex,
    RegisteredEngine::ExaMcp,
    RegisteredEngine::Startpage,
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
