mod arxiv;
mod bangs;
mod brave;
mod crossref;
mod duckduckgo_html;
mod github;
mod mojeek;
mod pubmed;
mod qwant;
mod registry;
mod semantic_scholar;
mod text;
mod wikipedia;
mod yahoo;

pub use bangs::{resolve_bangs, BangError, BangResolution};
pub use registry::{default_engine_ids, find_engine, registry, RegisteredEngine};
