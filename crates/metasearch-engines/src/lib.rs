mod arxiv;
mod brave;
mod crossref;
mod duckduckgo_html;
mod pubmed;
mod qwant;
mod registry;
mod semantic_scholar;
mod text;
mod wikipedia;

pub use registry::{default_engine_ids, find_engine, registry, RegisteredEngine};
